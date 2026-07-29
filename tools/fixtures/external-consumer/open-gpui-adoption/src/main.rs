// Shared source and archive external-consumer recipe.
use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeViewDescriptor, GraphBuilder,
    GraphId, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationRequest, MutationSettlement, OpenGpuiManagedMutation,
    OpenGpuiNodeBinding, OpenGpuiPortHandle,
};
use jellyflow_open_gpui::chrome::{
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePainterRecipe,
    OpenGpuiEdgePainterRegistry, OpenGpuiEdgeStrokePattern, OpenGpuiEdgeTone,
};
use jellyflow_open_gpui::host::{OpenGpuiSceneHost, OpenGpuiSceneHostDiagnosticKind};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistry,
    OpenGpuiNodeRegistryRegistrationError, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};
use jellyflow_open_gpui::theme::OpenGpuiGraphTheme;
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{PublishedPortOutcome, RendererStamp, ViewSessionOptions};
use open_gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, div, px, rgb,
};
use open_gpui_ui_components::{Button, ButtonVariant, ListboxOption, Select, TextInput};

const KIND: &str = "external.dynamic-workflow";
const RENDERER: &str = "external.native.dynamic-workflow";
const SINK_KIND: &str = "external.sink";
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
    mutation: OpenGpuiManagedMutation<()>,
}

impl ProductNode {
    fn add_dynamic_port(&mut self, cx: &mut Context<Self>) {
        self.state
            .click_count
            .set(self.state.click_count.get().saturating_add(1));
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("command callback opens a current authoring view");
        if authoring.port(dynamic_port_id()).is_some() {
            return;
        }
        let base = authoring
            .port(base_port_id())
            .expect("base port remains in the authoring view")
            .facts()
            .clone();
        let mut dynamic = base.clone();
        dynamic.key = PortKey::new("dynamic-result");
        let command = GraphCommand::topology(
            authoring.version(),
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
        self.mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(()),
                MutationLayoutEffect::InvalidateWhenSatisfied,
                cx,
            )
            .expect("managed mutation admits the dynamic-port command");
        cx.notify();
    }
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
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("retained node opens one coherent authoring view");
        if matches!(
            self.mutation.observe(
                &authoring,
                |_| authoring.port(dynamic_port_id()).is_some(),
                cx,
            ),
            MutationSettlement::Satisfied { .. }
        ) {
            self.state
                .command_count
                .set(self.state.command_count.get().saturating_add(1));
        }
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
        let product_node = cx.weak_entity();
        let drag_header = self.binding.read(cx).drag_handle().wrap(
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
                            let _ = product_node.update(cx, |node, cx| {
                                node.add_dynamic_port(cx);
                            });
                        })
                        .child(self.state.title.clone()),
                ),
            )
            .child(
                div()
                    .id("external-product-scroll")
                    .absolute()
                    .left(px(16.0))
                    .top(px(48.0))
                    .w(px(156.0))
                    .h(px(48.0))
                    .overflow_y_scroll()
                    .child(
                        TextInput::new("external-product-input", "Node label")
                            .value(self.state.title.clone())
                            .placeholder("Node label"),
                    )
                    .child(
                        Select::new("external-product-select", "Node mode")
                            .selected("data")
                            .option(ListboxOption::new("data", "Data")),
                    )
                    .child(
                        Button::new("external-product-button", "Add dynamic port")
                            .variant(ButtonVariant::Secondary),
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
        self.binding.read(cx).measure_content(root)
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

fn product_node_state() -> Rc<ProductNodeState> {
    Rc::new(ProductNodeState {
        title: "Application-owned dynamic node".to_owned(),
        mount_count: Cell::new(0),
        render_count: Cell::new(0),
        click_count: Cell::new(0),
        command_count: Cell::new(0),
        connected_state_seen: Cell::new(false),
    })
}

fn external_catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND)
                .port(NodePortSpec::new(
                    "base",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("register external dynamic node kind");
    catalog
        .register(
            NodeKindSpec::builder(SINK_KIND)
                .port(NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .expect("register external sink kind");
    catalog
}

fn build_product_registry(
    state: Rc<ProductNodeState>,
    catalog: &NodeCatalog,
) -> OpenGpuiNodeRegistry {
    let factory_state = Rc::clone(&state);
    let mut registry = OpenGpuiNodeRegistry::builder();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(108.0, 160.0).expect("external content-height policy"),
        ),
    );
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                KIND,
                1,
                RendererStamp::new(RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Dynamic workflow"),
                move |binding, _window, cx| {
                    let state = Rc::clone(&factory_state);
                    state
                        .mount_count
                        .set(state.mount_count.get().saturating_add(1));
                    let mutation = binding.read(cx).managed_mutation();
                    Ok(cx.new(|_| ProductNode {
                        binding,
                        state,
                        mutation,
                    }))
                },
            )
            .with_policy(policy),
        )
        .expect("unique external renderer descriptor");
    registry
        .seal(catalog)
        .expect("external renderer registrations must be valid")
}

fn assert_invalid_registry_is_rejected(catalog: &NodeCatalog) {
    let state = product_node_state();
    let mut registry = OpenGpuiNodeRegistry::builder();
    let error = registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new("   ", 1),
            OpenGpuiNodeAdapterPresentation::new("Invalid renderer"),
            move |binding, _window, cx| {
                let mutation = binding.read(cx).managed_mutation();
                Ok(cx.new(|_| ProductNode {
                    binding,
                    state: Rc::clone(&state),
                    mutation,
                }))
            },
        ))
        .expect_err("whitespace renderer stamp must be rejected before builder mutation");
    assert!(matches!(
        error,
        OpenGpuiNodeRegistryRegistrationError::EmptyRendererKey { .. }
    ));
    assert!(
        registry
            .seal(catalog)
            .expect("rejected descriptor leaves the registry empty")
            .is_empty()
    );
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

#[cfg(test)]
#[open_gpui::test]
fn external_retained_node_mounts_and_measures_real_ports(cx: &mut open_gpui::TestAppContext) {
    let kind = NodeKindKey::new(KIND);
    let node = node_id();
    let base_port = base_port_id();
    let dynamic_port = dynamic_port_id();
    let catalog = external_catalog();
    let sink_kind = NodeKindKey::new(SINK_KIND);

    let creation = catalog
        .instantiate_node_with_ids(
            &kind,
            node,
            CanvasRect {
                origin: CanvasPoint { x: 40.0, y: 48.0 },
                size: CanvasSize {
                    width: 260.0,
                    height: 180.0,
                },
            },
            [base_port],
        )
        .expect("instantiate external dynamic node");
    let (created_id, created_node, created_ports) = creation.into_parts();
    let sink_creation = catalog
        .instantiate_node_with_ids(
            &sink_kind,
            sink_node_id(),
            CanvasRect {
                origin: CanvasPoint { x: 480.0, y: 48.0 },
                size: CanvasSize {
                    width: 160.0,
                    height: 96.0,
                },
            },
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
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph.build().expect("external fixture graph must validate"))
        .expect("create external graph runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach external view");
    let weak_view = view.downgrade();

    let state = product_node_state();
    assert_invalid_registry_is_rejected(&catalog);
    let registry = build_product_registry(Rc::clone(&state), &catalog);
    assert_eq!(
        registry.renderer_stamp(&kind, 1),
        Some(&RendererStamp::new(RENDERER, 1))
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
        assert_eq!(host.resource_snapshot().active_nodes(), 2);
        let diagnostic = host
            .last_diagnostic()
            .expect("the deliberately missing sink renderer uses the fallback");
        assert_eq!(
            diagnostic.kind(),
            OpenGpuiSceneHostDiagnosticKind::MissingRenderer
        );
        assert_eq!(diagnostic.node(), Some(sink_node_id()));
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

#[cfg(test)]
fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn main() {
    let catalog = external_catalog();
    assert_invalid_registry_is_rejected(&catalog);
    let registry = build_product_registry(product_node_state(), &catalog);
    assert_eq!(
        registry.renderer_stamp(&NodeKindKey::new(KIND), 1),
        Some(&RendererStamp::new(RENDERER, 1))
    );
}
