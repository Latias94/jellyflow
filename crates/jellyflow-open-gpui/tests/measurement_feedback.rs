use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationRequest, MutationSettlement, OpenGpuiManagedMutation,
    OpenGpuiNodeBinding,
};
use jellyflow_open_gpui::host::{
    InitialViewportPolicy, OpenGpuiSceneHost, OpenGpuiSceneHostDiagnosticKind,
};
use jellyflow_open_gpui::interaction::{OpenGpuiPortDragPhase, OpenGpuiPortEvent};
use jellyflow_open_gpui::registry::{OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistry};
use jellyflow_open_gpui::scale_policy::OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM;
use jellyflow_open_gpui::size_policy::{
    OpenGpuiContentHeightPolicy, OpenGpuiNodeSizeConvergence, OpenGpuiNodeSizePolicy,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, graph::GraphRuntime, view::PublishedPortOutcome, view::ViewCloseReason,
    view::ViewSessionOptions, viewport::ViewportTransform,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, MouseButton, ParentElement, Render, Styled,
    Window, div, point, px,
};

const KIND: &str = "test.measured-node";
const RENDERER: &str = "test.native.measured-node";
const VALID_SIBLING_KIND: &str = "test.valid-sibling";
const VALID_SIBLING_RENDERER: &str = "test.native.valid-sibling";
const FIXED_NATIVE_CONTROL_SELECTOR: &str = "jellyflow-test-fixed-native-control";
const OVERVIEW_NODE_SELECTOR: &str = "jellyflow-overview-node";
const CONTENT_MASK_NODE_SELECTOR: &str = "jellyflow-content-mask-node";
const CONTENT_MASK_OUTPUT_SELECTOR: &str = "jellyflow-content-mask-output-port";

struct MeasuredPortsNode {
    binding: Entity<OpenGpuiNodeBinding>,
    measure_output: Rc<Cell<bool>>,
    mutate_runtime_once: Option<Rc<GraphRuntime>>,
}

struct OffsetScene {
    host: Entity<OpenGpuiSceneHost>,
}

struct FixedIntrinsicChildNode {
    binding: Entity<OpenGpuiNodeBinding>,
    renders: Rc<Cell<usize>>,
}

struct RepeatedPrepaintPortNode {
    binding: Entity<OpenGpuiNodeBinding>,
    duplicate_input: Rc<Cell<bool>>,
}

struct ContentMaskedPortsNode {
    binding: Entity<OpenGpuiNodeBinding>,
    output_top: Rc<Cell<f32>>,
    output_events: Rc<Cell<u32>>,
}

struct TransactionalMeasuredPortsNode {
    binding: Entity<OpenGpuiNodeBinding>,
    measure_output: Rc<Cell<bool>>,
    output_top: Rc<Cell<f32>>,
}

struct ContentMeasuredNode {
    binding: Entity<OpenGpuiNodeBinding>,
    height: Rc<Cell<f32>>,
}

struct PrefixQueuedContentNode {
    binding: Entity<OpenGpuiNodeBinding>,
    mutation: OpenGpuiManagedMutation<serde_json::Value>,
    render_count: usize,
}

struct AlternatingContentNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

struct StaleContentMeasuredNode {
    binding: Entity<OpenGpuiNodeBinding>,
    runtime: Rc<GraphRuntime>,
    render_count: usize,
}

struct RuntimeDroppingIncompleteNode {
    binding: Entity<OpenGpuiNodeBinding>,
    runtime_owner: Rc<RefCell<Option<Rc<GraphRuntime>>>>,
}

struct ValidSiblingNode {
    binding: Entity<OpenGpuiNodeBinding>,
    mutation: OpenGpuiManagedMutation<serde_json::Value>,
    last_settlement: Option<MutationSettlement>,
}

impl Render for OffsetScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div()
            .size_full()
            .pt(px(31.0))
            .pl(px(47.0))
            .child(self.host.clone())
    }
}

impl Render for FixedIntrinsicChildNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.renders.set(self.renders.get().saturating_add(1));
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live authoring view");
        let input = authoring
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("output belongs to this node")
            .handle()
            .clone();

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .debug_selector(|| FIXED_NATIVE_CONTROL_SELECTOR.to_owned())
                    .w(px(220.0))
                    .h(px(120.0))
                    .child("fixed control"),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(53.0))
                    .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(53.0))
                    .child(output.measure(div().w(px(14.0)).h(px(14.0)))),
            )
    }
}

impl Render for RepeatedPrepaintPortNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live authoring view");
        let input = authoring
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("output belongs to this node")
            .handle()
            .clone();

        let mut node = div()
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(53.0))
                    .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
            )
            .child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(53.0))
                    .child(output.measure(div().w(px(14.0)).h(px(14.0)))),
            );
        if self.duplicate_input.get() {
            node = node.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(40.0))
                    .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
            );
        }
        node
    }
}

impl Render for ContentMaskedPortsNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live authoring view");
        let input = authoring
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("output belongs to this node")
            .handle()
            .clone();
        let output_top = self.output_top.get();
        let output_events = Rc::clone(&self.output_events);
        let output_marker = div()
            .debug_selector(|| CONTENT_MASK_OUTPUT_SELECTOR.to_owned())
            .w(px(14.0))
            .h(px(14.0));

        div()
            .debug_selector(|| CONTENT_MASK_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(53.0))
                    .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
            )
            .child(div().absolute().right(px(0.0)).top(px(output_top)).child(
                output.interactive_with_observer(output_marker, move |event, _, _| {
                    if matches!(
                        event,
                        OpenGpuiPortEvent::Pointer(event)
                            if event.phase() == OpenGpuiPortDragPhase::Start
                    ) {
                        output_events.set(output_events.get().saturating_add(1));
                    }
                }),
            ))
    }
}

impl Render for TransactionalMeasuredPortsNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live authoring view");
        let input = authoring
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("output belongs to this node")
            .handle()
            .clone();

        let mut node = div().relative().size_full().child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(40.0))
                .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
        );
        if self.measure_output.get() {
            node = node.child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(self.output_top.get()))
                    .child(output.measure(div().w(px(14.0)).h(px(14.0)))),
            );
        }
        node
    }
}

impl Render for ContentMeasuredNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        binding.measure_content(
            div()
                .w(px(220.0))
                .h(px(self.height.get()))
                .child("content-authoritative node"),
        )
    }
}

impl Render for PrefixQueuedContentNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.render_count = self.render_count.saturating_add(1);
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("prefix fixture binding stays live");
        let _ = self
            .mutation
            .observe(&authoring, |intent| authoring.node().data == *intent, cx);
        if self.render_count == 2 {
            let intent = serde_json::json!({ "prefix": true });
            let command = GraphCommand::set_node_data(
                authoring.version(),
                NodeId::from_u128(10),
                intent.clone(),
            );
            self.mutation
                .dispatch(
                    authoring,
                    command,
                    MutationRequest::new(intent),
                    MutationLayoutEffect::Preserve,
                    cx,
                )
                .expect("prefix fixture command enters managed authoring");
        }
        self.binding.read(cx).measure_content(
            div()
                .w(px(220.0))
                .h(px(176.0))
                .child("prefix-ordered content node"),
        )
    }
}

impl Render for AlternatingContentNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let height = binding
            .authoring()
            .expect("alternating fixture binding stays live")
            .node()
            .size
            .expect("alternating fixture has an authoritative size")
            .height;
        let measured_height = if height < 130.0 { 140.0 } else { 120.0 };
        binding.measure_content(
            div()
                .w(px(220.0))
                .h(px(measured_height))
                .child("alternating content node"),
        )
    }
}

impl Render for StaleContentMeasuredNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.render_count = self.render_count.saturating_add(1);
        if self.render_count == 2 {
            let version = self
                .runtime
                .snapshot()
                .expect("stale fixture snapshot")
                .version();
            self.runtime
                .execute(GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "height": 244.0 }),
                ))
                .into_result()
                .expect("stale fixture updates the same node");
        }
        let binding = self.binding.read(cx);
        let height = binding
            .authoring()
            .expect("stale fixture binding stays live")
            .node()
            .data
            .get("height")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(176.0) as f32;
        binding.measure_content(div().w(px(220.0)).h(px(height)).child("stale content node"))
    }
}

impl Render for RuntimeDroppingIncompleteNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let input = self
            .binding
            .read(cx)
            .authoring()
            .expect("authoring opens before the runtime is dropped")
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        drop(self.runtime_owner.borrow_mut().take());
        div()
            .size_full()
            .child(input.measure(div().w(px(14.0)).h(px(14.0))))
    }
}

impl Render for ValidSiblingNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("valid sibling binding stays live");
        let settlement =
            self.mutation
                .observe(&authoring, |intent| authoring.node().data == *intent, cx);
        if !matches!(
            settlement,
            MutationSettlement::Ready | MutationSettlement::Pending { .. }
        ) {
            self.last_settlement = Some(settlement);
        }
        div().size_full().child("valid sibling renderer")
    }
}

impl ValidSiblingNode {
    fn set_data(&mut self, value: serde_json::Value, cx: &mut Context<Self>) {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("valid sibling binding stays live");
        let command =
            GraphCommand::set_node_data(authoring.version(), NodeId::from_u128(11), value.clone());
        self.mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(value),
                MutationLayoutEffect::Preserve,
                cx,
            )
            .expect("valid sibling command enters managed authoring");
    }
}

impl Render for MeasuredPortsNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        if let Some(runtime) = self.mutate_runtime_once.take() {
            runtime
                .execute(GraphCommand::set_node_data(
                    runtime.snapshot().unwrap().version(),
                    NodeId::from_u128(10),
                    serde_json::json!({ "label": "updated elsewhere" }),
                ))
                .into_result()
                .unwrap();
        }
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live authoring view");
        let input = authoring
            .port(input_id())
            .expect("input belongs to this node")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("output belongs to this node")
            .handle()
            .clone();

        let mut node = div()
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            .child(input.measure(div().w(px(14.0)).h(px(14.0))))
            .child("native controls");
        if self.measure_output.get() {
            node = node.child(output.measure(div().w(px(14.0)).h(px(14.0))));
        }
        node
    }
}

#[open_gpui::test]
fn measured_surface_bootstrap_requests_a_second_frame_and_mounts_visible_nodes(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });

    let host_slot = Rc::new(RefCell::new(None));
    let installed_host = Rc::clone(&host_slot);
    let (_, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new_measured_surface(
                view,
                registry.seal().expect("valid test registry"),
            )
        });
        *installed_host.borrow_mut() = Some(host.clone());
        OffsetScene { host }
    });
    let host = host_slot.borrow_mut().take().expect("installed host");
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
    });
}

#[open_gpui::test]
fn measured_initial_fit_applies_once_and_yields_to_later_user_navigation(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let initial = view.viewport_transform().unwrap();
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new_measured_surface(view, registry.seal().expect("valid test registry"))
            .with_initial_viewport_policy(InitialViewportPolicy::FitOnFirstMeasuredGeometry)
    });
    draw(cx);

    let fitted = weak_view.viewport_transform().unwrap();
    assert_ne!(fitted, initial);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.initial_viewport_policy(),
            InitialViewportPolicy::Preserve
        );
    });

    let user_viewport = ViewportTransform::new(CanvasPoint { x: -41.0, y: 17.0 }, 1.2).unwrap();
    weak_view.set_viewport(user_viewport).unwrap();
    draw(cx);

    assert_eq!(weak_view.viewport_transform().unwrap(), user_viewport);
}

#[open_gpui::test]
fn measured_initial_fit_never_overwrites_a_user_owned_viewport(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let user_viewport = ViewportTransform::new(CanvasPoint { x: 73.0, y: -29.0 }, 1.35).unwrap();
    view.set_viewport(user_viewport).unwrap();
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });

    let (_, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new_measured_surface(view, registry.seal().expect("valid test registry"))
            .with_initial_viewport_policy(InitialViewportPolicy::FitOnFirstMeasuredGeometry)
    });
    draw(cx);

    assert_eq!(weak_view.viewport_transform().unwrap(), user_viewport);
}

#[open_gpui::test]
fn native_port_handles_publish_complete_geometry_after_prepaint(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint { x: 7.0, y: -3.0 }, 2.0)
            .expect("valid measured-node viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();
    let measure_output = Rc::new(Cell::new(true));
    let factory_measure_output = measure_output.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: factory_measure_output.clone(),
            mutate_runtime_once: None,
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let geometry = weak_view.geometry_snapshot().unwrap();
    let node = geometry.node(NodeId::from_u128(10)).expect("node geometry");
    assert_eq!(node.bounds().origin, CanvasPoint { x: 20.0, y: 20.0 });
    assert_eq!(
        node.bounds().size,
        CanvasSize {
            width: 110.0,
            height: 60.0,
        },
    );
    let input = geometry.port(input_id()).expect("input geometry");
    let output = geometry.port(output_id()).expect("output geometry");
    assert_eq!(input.node(), NodeId::from_u128(10));
    assert_eq!(output.node(), NodeId::from_u128(10));
    assert!(input.visual_bounds().size.is_positive_finite());
    assert!(output.visual_bounds().size.is_positive_finite());
    assert_close(input.visual_bounds().origin.x, node.bounds().origin.x);
    assert_close(
        output.visual_bounds().origin.x + output.visual_bounds().size.width,
        node.bounds().origin.x + node.bounds().size.width,
    );

    cx.read_entity(&host, |host, _| assert!(host.last_diagnostic().is_none()));

    let settled_revision = geometry.layout_revision();
    for _ in 0..120 {
        draw(cx);
    }
    assert_eq!(
        weak_view.geometry_snapshot().unwrap().layout_revision(),
        settled_revision,
    );

    measure_output.set(false);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    let geometry = weak_view.geometry_snapshot().unwrap();
    let node = geometry
        .node(NodeId::from_u128(10))
        .expect("measured node root remains published");
    assert!(node.is_fresh());
    assert!(geometry.port(input_id()).is_some());
    assert_eq!(geometry.layout_revision(), settled_revision);
    assert!(geometry.port(output_id()).is_some());
    assert!(matches!(
        geometry.port_outcome(output_id()),
        Some(PublishedPortOutcome::Rendered(_)),
    ));
    cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("incomplete port coverage is explicit");
        assert!(diagnostic.message().contains("missing real ports"));
    });
}

#[open_gpui::test]
fn native_content_mask_removes_and_restores_only_port_interaction_geometry(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let output_top = Rc::new(Cell::new(53.0));
    let output_events = Rc::new(Cell::new(0));
    let captured_node = Rc::new(RefCell::new(None::<Entity<ContentMaskedPortsNode>>));

    let mut registry = OpenGpuiNodeRegistry::builder();
    let factory_output_top = Rc::clone(&output_top);
    let factory_output_events = Rc::clone(&output_events);
    let factory_node = Rc::clone(&captured_node);
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = cx.new(|_| ContentMaskedPortsNode {
            binding,
            output_top: Rc::clone(&factory_output_top),
            output_events: Rc::clone(&factory_output_events),
        });
        *factory_node.borrow_mut() = Some(node.clone());
        node
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid content-mask registry"),
            viewport(),
        )
    });
    draw(cx);

    let visible = weak_view.geometry_snapshot().unwrap();
    let visible_output = visible.port(output_id()).expect("visible output geometry");
    assert_eq!(visible_output.port(), output_id());
    assert!(visible_output.hit_bounds().is_some());
    let visible_marker = cx
        .debug_bounds(CONTENT_MASK_OUTPUT_SELECTOR)
        .expect("visible output marker");
    cx.simulate_mouse_move(visible_marker.center(), None, Default::default());
    draw(cx);
    let node = captured_node
        .borrow()
        .clone()
        .expect("captured content-mask node");
    cx.read_entity(&node, |node, cx| {
        assert!(
            node.binding
                .read(cx)
                .presentation()
                .port(output_id())
                .expect("output presentation")
                .hovered(),
            "visible native hitbox establishes port hover",
        );
    });

    output_top.set(114.0);
    cx.update_entity(&node, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .unwrap()
        );
    });
    draw(cx);
    cx.read_entity(&node, |node, cx| {
        assert!(
            !node
                .binding
                .read(cx)
                .presentation()
                .port(output_id())
                .expect("output presentation")
                .hovered(),
            "a partially clipped replacement hitbox clears stale hover without a mouse move",
        );
    });
    let partial_output = weak_view
        .geometry_snapshot()
        .unwrap()
        .port(output_id())
        .expect("partially clipped output keeps interaction geometry")
        .clone();
    assert!(partial_output.hit_bounds().is_some());
    let partial_node_bounds = cx
        .debug_bounds(CONTENT_MASK_NODE_SELECTOR)
        .expect("content-mask node bounds");
    let partial_marker_bounds = cx
        .debug_bounds(CONTENT_MASK_OUTPUT_SELECTOR)
        .expect("partially clipped marker bounds");
    let partial_node_bottom = partial_node_bounds.origin.y + partial_node_bounds.size.height;
    assert!(partial_marker_bounds.origin.y < partial_node_bottom);
    assert!(
        partial_marker_bounds.origin.y + partial_marker_bounds.size.height > partial_node_bottom,
        "the regression must exercise partial rather than complete clipping",
    );

    output_top.set(121.0);
    cx.update_entity(&node, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .unwrap()
        );
    });
    draw(cx);
    cx.read_entity(&node, |node, cx| {
        assert!(
            !node
                .binding
                .read(cx)
                .presentation()
                .port(output_id())
                .expect("output presentation")
                .hovered(),
            "clipping clears prior hover without waiting for another pointer move",
        );
    });

    let clipped = weak_view.geometry_snapshot().unwrap();
    let clipped_output = clipped
        .port(output_id())
        .expect("clipped output keeps published route geometry");
    assert_eq!(clipped_output.port(), visible_output.port());
    assert_eq!(clipped_output.hit_bounds(), None);
    assert!(clipped_output.visual_bounds().size.is_positive_finite());
    assert!(clipped_output.route_anchor().is_finite());

    let node_bounds = cx
        .debug_bounds(CONTENT_MASK_NODE_SELECTOR)
        .expect("content-mask node bounds");
    let marker_bounds = cx
        .debug_bounds(CONTENT_MASK_OUTPUT_SELECTOR)
        .expect("fully clipped marker keeps layout bounds");
    let node_bottom = node_bounds.origin.y + node_bounds.size.height;
    assert!(marker_bounds.origin.y > node_bottom);
    let clipped_hit_slop = point(marker_bounds.center().x, node_bottom - px(1.0));
    assert!(clipped_hit_slop.y < marker_bounds.origin.y);

    cx.simulate_mouse_move(clipped_hit_slop, None, Default::default());
    draw(cx);
    cx.read_entity(&node, |node, cx| {
        assert!(
            !node
                .binding
                .read(cx)
                .presentation()
                .port(output_id())
                .expect("output presentation")
                .hovered(),
            "a fully clipped marker cannot acquire hover through protruding hit slop",
        );
    });

    cx.simulate_mouse_down(clipped_hit_slop, MouseButton::Left, Default::default());
    assert_eq!(output_events.get(), 0);
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
    });
    cx.simulate_mouse_up(clipped_hit_slop, MouseButton::Left, Default::default());
    cx.run_until_parked();

    output_top.set(53.0);
    cx.update_entity(&node, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .unwrap()
        );
    });
    draw(cx);

    let restored = weak_view.geometry_snapshot().unwrap();
    let restored_output = restored
        .port(output_id())
        .expect("restored output keeps the same port id");
    assert_eq!(restored_output.port(), visible_output.port());
    assert!(restored_output.hit_bounds().is_some());
    assert_eq!(
        restored_output.route_anchor(),
        visible_output.route_anchor()
    );

    let restored_marker = cx
        .debug_bounds(CONTENT_MASK_OUTPUT_SELECTOR)
        .expect("restored output marker");
    cx.simulate_mouse_down(
        restored_marker.center(),
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(output_events.get(), 1);
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.active_port_drag().map(|owner| owner.port()),
            Some(output_id())
        );
    });

    output_top.set(114.0);
    cx.update_entity(&node, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .unwrap()
        );
    });
    draw(cx);

    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.active_port_drag().map(|owner| owner.port()),
            Some(output_id()),
            "replacing a captured port with a partially clipped hitbox must preserve the drag",
        );
    });
    assert!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(output_id())
            .expect("partially clipped active port keeps route geometry")
            .hit_bounds()
            .is_some(),
    );

    output_top.set(121.0);
    cx.update_entity(&node, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .unwrap()
        );
    });
    draw(cx);

    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.active_port_drag().map(|owner| owner.port()),
            Some(output_id()),
            "clipping prevents new hits without cancelling the captured connection drag",
        );
    });
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(output_id())
            .expect("clipped active port keeps route geometry")
            .hit_bounds(),
        None,
    );

    cx.simulate_mouse_up(
        restored_marker.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
    });
}

#[open_gpui::test]
fn incomplete_layout_keeps_command_and_authoritative_route_until_atomic_refresh(
    cx: &mut open_gpui::TestAppContext,
) {
    let mut node_catalog = catalog();
    node_catalog
        .register(
            NodeKindSpec::builder(VALID_SIBLING_KIND, "Valid sibling", VALID_SIBLING_RENDERER)
                .default_size(CanvasSize {
                    width: 140.0,
                    height: 80.0,
                })
                .build(),
        )
        .expect("unique sibling kind");
    let runtime = GraphRuntime::builder(node_catalog)
        .build(graph_with_route_and_valid_sibling())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("routed view");
    let weak_view = view.downgrade();
    let measure_output = Rc::new(Cell::new(true));
    let output_top = Rc::new(Cell::new(40.0));
    let sibling_node = Rc::new(RefCell::new(None));

    let mut registry = OpenGpuiNodeRegistry::builder();
    let factory_measure_output = Rc::clone(&measure_output);
    let factory_output_top = Rc::clone(&output_top);
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| TransactionalMeasuredPortsNode {
            binding,
            measure_output: Rc::clone(&factory_measure_output),
            output_top: Rc::clone(&factory_output_top),
        })
    });
    let captured_sibling = Rc::clone(&sibling_node);
    registry.register(VALID_SIBLING_RENDERER, move |binding, _window, cx| {
        let mutation = binding.read(cx).managed_mutation();
        let node = cx.new(|_| ValidSiblingNode {
            binding,
            mutation,
            last_settlement: None,
        });
        *captured_sibling.borrow_mut() = Some(node.clone());
        node
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid routed registry"),
            viewport(),
        )
    });
    draw(cx);

    let authoritative = weak_view
        .geometry_snapshot()
        .expect("initial authoritative geometry");
    let node = NodeId::from_u128(10);
    assert!(authoritative.node(node).is_some_and(|node| node.is_fresh()));
    assert!(authoritative.port(input_id()).is_some());
    assert!(authoritative.port(output_id()).is_some());
    assert!(authoritative.route(edge_id()).is_some());

    measure_output.set(false);
    output_top.set(72.0);
    let sibling = sibling_node
        .borrow()
        .clone()
        .expect("captured valid sibling node");
    cx.update_entity(&sibling, |node, cx| {
        node.set_data(serde_json::json!({ "same-frame": "applied" }), cx);
    });
    draw(cx);

    cx.read_entity(&sibling, |node, _| {
        assert!(matches!(
            node.last_settlement,
            Some(MutationSettlement::Satisfied { .. })
        ));
    });
    assert_eq!(
        runtime
            .snapshot()
            .expect("graph after incomplete layout")
            .graph()
            .node(&NodeId::from_u128(11))
            .expect("valid sibling")
            .data,
        serde_json::json!({ "same-frame": "applied" })
    );
    let incomplete = weak_view
        .geometry_snapshot()
        .expect("geometry after incomplete layout");
    assert_eq!(incomplete.node(node), authoritative.node(node));
    assert_eq!(incomplete.port(input_id()), authoritative.port(input_id()));
    assert_eq!(
        incomplete.port(output_id()),
        authoritative.port(output_id())
    );
    assert_eq!(incomplete.route(edge_id()), authoritative.route(edge_id()));
    cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("incomplete layout diagnostic");
        assert!(diagnostic.message().contains("missing real ports"));
    });

    measure_output.set(true);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    let refreshed = weak_view
        .geometry_snapshot()
        .expect("complete refreshed geometry");
    assert!(refreshed.layout_revision() > incomplete.layout_revision());
    assert!(refreshed.node(node).is_some_and(|node| node.is_fresh()));
    assert_eq!(refreshed.port(input_id()), authoritative.port(input_id()));
    assert_ne!(refreshed.port(output_id()), authoritative.port(output_id()));
    assert_ne!(refreshed.route(edge_id()), authoritative.route(edge_id()));
}

#[open_gpui::test]
fn content_height_policy_converges_through_revisioned_geometry_commands(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(content_graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let height = Rc::new(Cell::new(176.0));
    let factory_height = height.clone();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid content-height bounds"),
        ),
    );

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, move |binding, _window, cx| {
        cx.new(|_| ContentMeasuredNode {
            binding,
            height: factory_height.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let snapshot = weak_view.graph_snapshot().expect("converged graph");
    let node = snapshot
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("content-authoritative node");
    assert_eq!(
        node.size,
        Some(CanvasSize {
            width: 220.0,
            height: 176.0,
        })
    );
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        0,
        "content-derived geometry must not create a user-facing undo step"
    );

    height.set(244.0);
    cx.update_entity(&host, |host, cx| {
        assert!(
            host.invalidate_node_layout(NodeId::from_u128(10), cx)
                .expect("content invalidation")
        );
    });
    draw(cx);

    let snapshot = weak_view.graph_snapshot().expect("resized graph");
    assert_eq!(
        snapshot
            .graph()
            .node(&NodeId::from_u128(10))
            .expect("content-authoritative node")
            .size,
        Some(CanvasSize {
            width: 220.0,
            height: 244.0,
        })
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    let settled_graph_revision = snapshot.revision();
    let settled_layout_revision = weak_view
        .geometry_snapshot()
        .expect("settled geometry")
        .layout_revision();

    for _ in 0..120 {
        draw(cx);
    }
    assert_eq!(
        weak_view.graph_snapshot().unwrap().revision(),
        settled_graph_revision
    );
    assert_eq!(
        weak_view.geometry_snapshot().unwrap().layout_revision(),
        settled_layout_revision
    );
}

#[open_gpui::test]
fn content_size_waits_for_queued_prefix_and_binds_one_fresh_revision(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(content_graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid content-height bounds"),
        ),
    );
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, |binding, _window, cx| {
        let mutation = binding.read(cx).managed_mutation();
        cx.new(|_| PrefixQueuedContentNode {
            binding,
            mutation,
            render_count: 0,
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    draw(cx);

    let snapshot = runtime.snapshot().expect("prefix and size commands settle");
    let node = snapshot
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("content node remains present");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(node.data, serde_json::json!({ "prefix": true }));
    assert_eq!(
        node.size,
        Some(CanvasSize {
            width: 220.0,
            height: 176.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.last_diagnostic().is_none());
    });
}

#[open_gpui::test]
fn host_alternating_content_freezes_after_exact_attempt_bound(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(content_graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let convergence =
        OpenGpuiNodeSizeConvergence::new(0.5, 2, 4).expect("valid bounded convergence fixture");
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0)
                .expect("valid content-height bounds")
                .with_convergence(convergence),
        ),
    );
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, |binding, _window, cx| {
        cx.new(|_| AlternatingContentNode { binding })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    draw(cx);

    assert_eq!(runtime.snapshot().expect("frozen graph").revision(), 4);
    cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("alternating convergence emits one diagnostic");
        assert_eq!(diagnostic.kind(), OpenGpuiSceneHostDiagnosticKind::Feedback);
        assert!(diagnostic.message().contains("reason=AlternatingCycle"));
        assert!(diagnostic.message().contains("attempts=4"));
    });

    let settled_revision = runtime.snapshot().expect("settled graph").revision();
    for _ in 0..120 {
        draw(cx);
    }
    assert_eq!(
        runtime
            .snapshot()
            .expect("quiescent frozen graph")
            .revision(),
        settled_revision
    );
}

#[open_gpui::test]
fn content_sizes_from_one_frame_commit_as_one_atomic_geometry_revision(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_content_sibling())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let height = Rc::new(Cell::new(176.0));
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid content-height bounds"),
        ),
    );

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, move |binding, _window, cx| {
        cx.new(|_| ContentMeasuredNode {
            binding,
            height: height.clone(),
        })
    });
    let (_host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    draw(cx);

    let snapshot = runtime.snapshot().expect("atomically resized graph");
    assert_eq!(snapshot.revision(), 1);
    for node in [NodeId::from_u128(10), NodeId::from_u128(11)] {
        assert_eq!(
            snapshot.graph().node(&node).expect("content node").size,
            Some(CanvasSize {
                width: 220.0,
                height: 176.0,
            })
        );
    }
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[open_gpui::test]
fn stale_prepaint_feedback_refreshes_without_becoming_a_user_diagnostic(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let factory_runtime = runtime.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: Some(factory_runtime.clone()),
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.port(input_id()).is_some());
    assert!(geometry.port(output_id()).is_some());
    cx.read_entity(&host, |host, _| {
        assert!(host.last_diagnostic().is_none());
    });
}

#[open_gpui::test]
fn stale_same_node_measurement_does_not_emit_a_size_command(cx: &mut open_gpui::TestAppContext) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(content_graph())
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid content-height bounds"),
        ),
    );
    let factory_runtime = Rc::clone(&runtime);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, move |binding, _window, cx| {
        cx.new(|_| StaleContentMeasuredNode {
            binding,
            runtime: Rc::clone(&factory_runtime),
            render_count: 0,
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    draw(cx);

    let snapshot = runtime.snapshot().expect("fresh content size settles");
    let node = snapshot
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("content node remains present");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(node.data, serde_json::json!({ "height": 244.0 }));
    assert_eq!(
        node.size,
        Some(CanvasSize {
            width: 220.0,
            height: 244.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.last_diagnostic().is_none());
    });
}

#[open_gpui::test]
fn nested_scene_origin_is_removed_from_native_layout_feedback(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint { x: 7.0, y: -3.0 }, 2.0)
            .expect("valid nested-scene viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });

    let captured_host = Rc::new(RefCell::new(None));
    let factory_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *factory_host.borrow_mut() = Some(host.clone());
        OffsetScene { host }
    });
    draw(cx);

    let geometry = weak_view.geometry_snapshot().unwrap();
    assert_eq!(
        geometry
            .node(NodeId::from_u128(10))
            .unwrap()
            .bounds()
            .origin,
        CanvasPoint { x: 20.0, y: 20.0 },
    );
    assert!(geometry.port(input_id()).is_some());
    assert!(geometry.port(output_id()).is_some());

    let host = captured_host.borrow().clone().expect("captured scene host");
    cx.read_entity(&host, |host, _| assert!(host.last_diagnostic().is_none()));
}

#[open_gpui::test]
fn missing_renderer_fallback_measures_every_real_port(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            OpenGpuiNodeRegistry::builder()
                .seal()
                .expect("valid empty test registry"),
            viewport(),
        )
    });
    draw(cx);

    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.port(input_id()).is_some());
    assert!(geometry.port(output_id()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.last_diagnostic().map(|diagnostic| diagnostic.kind()),
            Some(OpenGpuiSceneHostDiagnosticKind::MissingRenderer),
        );
    });
}

#[open_gpui::test]
fn duplicate_renderer_fallback_is_local_non_retryable_and_keeps_ports_interactive(
    cx: &mut open_gpui::TestAppContext,
) {
    let mut node_catalog = catalog();
    node_catalog
        .register(
            NodeKindSpec::builder(VALID_SIBLING_KIND, "Valid sibling", VALID_SIBLING_RENDERER)
                .default_size(CanvasSize {
                    width: 140.0,
                    height: 80.0,
                })
                .build(),
        )
        .unwrap();
    let runtime = GraphRuntime::builder(node_catalog)
        .build(graph_with_valid_sibling())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let first_factory_calls = Rc::new(Cell::new(0_u32));
    let second_factory_calls = Rc::new(Cell::new(0_u32));
    let valid_factory_calls = Rc::new(Cell::new(0_u32));

    let mut registry = OpenGpuiNodeRegistry::builder();
    let first_calls = first_factory_calls.clone();
    registry.register(RENDERER, move |binding, _window, cx| {
        first_calls.set(first_calls.get().saturating_add(1));
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });
    let second_calls = second_factory_calls.clone();
    registry.register(RENDERER, move |binding, _window, cx| {
        second_calls.set(second_calls.get().saturating_add(1));
        cx.new(|_| MeasuredPortsNode {
            binding,
            measure_output: Rc::new(Cell::new(true)),
            mutate_runtime_once: None,
        })
    });
    let valid_calls = valid_factory_calls.clone();
    registry.register(VALID_SIBLING_RENDERER, move |binding, _window, cx| {
        valid_calls.set(valid_calls.get().saturating_add(1));
        let mutation = binding.read(cx).managed_mutation();
        cx.new(|_| ValidSiblingNode {
            binding,
            mutation,
            last_settlement: None,
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal()
                .expect("duplicate non-empty key remains host-mountable"),
            viewport(),
        )
    });
    draw(cx);

    assert_eq!(first_factory_calls.get(), 0);
    assert_eq!(second_factory_calls.get(), 0);
    assert_eq!(valid_factory_calls.get(), 1);
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.node(NodeId::from_u128(11)).is_some());
    assert!(geometry.port(input_id()).is_some());
    let output = geometry
        .port(output_id())
        .expect("diagnostic fallback publishes the real output port");
    let output_bounds = output.visual_bounds();
    let output_center = point(
        px(output_bounds.origin.x + output_bounds.size.width / 2.0),
        px(output_bounds.origin.y + output_bounds.size.height / 2.0),
    );

    cx.read_entity(&host, |host, _| {
        let diagnostic = host.last_diagnostic().expect("duplicate diagnostic");
        assert_eq!(
            diagnostic.kind(),
            OpenGpuiSceneHostDiagnosticKind::InvalidRendererRegistration,
        );
        assert_eq!(diagnostic.node(), Some(NodeId::from_u128(10)));
        assert_eq!(diagnostic.renderer_key(), Some(RENDERER));
        assert_eq!(
            diagnostic.message(),
            "renderer key \"test.native.measured-node\" has duplicate registrations",
        );
    });
    assert!(!cx.update_entity(&host, |host, cx| {
        host.retry_mount(NodeId::from_u128(10), cx)
    }));

    cx.simulate_mouse_down(output_center, MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        let owner = host
            .active_port_drag()
            .expect("fallback port delegates connection ownership to the host");
        assert_eq!(owner.port(), output_id());
    });
    cx.simulate_mouse_up(output_center, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert_eq!(first_factory_calls.get(), 0);
    assert_eq!(second_factory_calls.get(), 0);
    assert_eq!(valid_factory_calls.get(), 1);
}

#[open_gpui::test]
fn duplicate_semantic_port_wrapper_preserves_authoritative_geometry(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();

    let duplicate_input = Rc::new(Cell::new(false));
    let factory_duplicate_input = Rc::clone(&duplicate_input);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RepeatedPrepaintPortNode {
            binding,
            duplicate_input: Rc::clone(&factory_duplicate_input),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let authoritative = weak_view.geometry_snapshot().unwrap();
    assert!(authoritative.port(input_id()).is_some());
    assert!(authoritative.port(output_id()).is_some());

    duplicate_input.set(true);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    let retained = weak_view.geometry_snapshot().unwrap();
    assert_eq!(
        retained.node(NodeId::from_u128(10)),
        authoritative.node(NodeId::from_u128(10))
    );
    assert_eq!(retained.port(input_id()), authoritative.port(input_id()));
    assert_eq!(retained.port(output_id()), authoritative.port(output_id()));
    cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("duplicate semantic wrapper is explicit");
        assert!(diagnostic.message().contains("duplicate real ports"));
    });
}

#[open_gpui::test]
fn semantic_zoom_uses_overview_below_one_and_native_pixels_at_or_above_one(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let renders = Rc::new(Cell::new(0));
    let factory_renders = renders.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| FixedIntrinsicChildNode {
            binding,
            renders: factory_renders.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    let pan = CanvasPoint { x: -11.0, y: 13.0 };
    for (zoom, native) in [(0.05, false), (0.5, false), (1.0, true), (2.0, true)] {
        weak_view
            .set_viewport(
                ViewportTransform::new(pan, zoom).expect("valid semantic-zoom viewport transform"),
            )
            .unwrap();
        draw(cx);

        let transform = weak_view.viewport_transform().unwrap();
        let geometry = weak_view.geometry_snapshot().unwrap();
        let node = geometry.node(NodeId::from_u128(10)).unwrap();
        let canvas_pixel_tolerance = 0.51 / zoom;
        assert_close_with(node.bounds().origin.x, 20.0, canvas_pixel_tolerance);
        assert_close_with(node.bounds().origin.y, 20.0, canvas_pixel_tolerance);
        let expected_canvas_size = if native {
            CanvasSize {
                width: 220.0 / zoom,
                height: 120.0 / zoom,
            }
        } else {
            CanvasSize {
                width: 220.0 / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
                height: 120.0 / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
            }
        };
        assert_close_with(
            node.bounds().size.width,
            expected_canvas_size.width,
            canvas_pixel_tolerance,
        );
        assert_close_with(
            node.bounds().size.height,
            expected_canvas_size.height,
            canvas_pixel_tolerance,
        );

        let node_screen_origin = transform
            .screen_point_for_canvas(node.bounds().origin)
            .expect("finite node screen origin");
        let expected_screen_size = if native {
            CanvasSize {
                width: 220.0,
                height: 120.0,
            }
        } else {
            CanvasSize {
                width: 220.0 * zoom / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
                height: 120.0 * zoom / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
            }
        };
        assert_close_with(
            node.bounds().size.width * zoom,
            expected_screen_size.width,
            0.51,
        );
        assert_close_with(
            node.bounds().size.height * zoom,
            expected_screen_size.height,
            0.51,
        );

        let input = geometry.port(input_id()).expect("real input geometry");
        let output = geometry.port(output_id()).expect("real output geometry");
        assert_hit_contains_visual(
            input.hit_bounds().expect("input is interactively visible"),
            input.visual_bounds(),
        );
        assert_hit_contains_visual(
            output
                .hit_bounds()
                .expect("output is interactively visible"),
            output.visual_bounds(),
        );
        assert_close(
            input.route_anchor().x,
            input.visual_bounds().origin.x + input.visual_bounds().size.width / 2.0,
        );
        assert_close(
            output.route_anchor().x,
            output.visual_bounds().origin.x + output.visual_bounds().size.width / 2.0,
        );

        let input_screen = transform
            .screen_point_for_canvas(input.route_anchor())
            .expect("finite input screen anchor");
        let output_screen = transform
            .screen_point_for_canvas(output.route_anchor())
            .expect("finite output screen anchor");
        let expected_port_y = if native {
            60.0
        } else {
            60.0 * zoom / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM
        };
        assert_close_with(input_screen.y - node_screen_origin.y, expected_port_y, 0.51);
        assert_close_with(
            output_screen.y - node_screen_origin.y,
            expected_port_y,
            0.51,
        );

        let native_control = cx.debug_bounds(FIXED_NATIVE_CONTROL_SELECTOR);
        let overview = cx.debug_bounds(OVERVIEW_NODE_SELECTOR);
        if native {
            let native_control = native_control.expect("native child is rendered at detail zoom");
            assert!(overview.is_none(), "zoom={zoom} must not render overview");
            assert_close(native_control.size.width.as_f32(), 220.0);
            assert_close(native_control.size.height.as_f32(), 120.0);
        } else {
            assert!(
                native_control.is_none(),
                "zoom={zoom} must not render fixed-pixel native children",
            );
            assert!(overview.is_some(), "zoom={zoom} must render overview");
        }
    }

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(host.last_diagnostic().is_none());
    });
    assert!(renders.get() > 0, "detail zoom rendered the retained child");
}

#[open_gpui::test]
fn semantic_zoom_holds_presentation_inside_the_hysteresis_gap(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 1.2)
            .expect("valid hysteresis viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();
    let renders = Rc::new(Cell::new(0));
    let factory_renders = renders.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| FixedIntrinsicChildNode {
            binding,
            renders: factory_renders.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });

    for (zoom, expects_detail) in [
        (1.2, true),
        (0.92, true),
        (0.86, true),
        (0.85, false),
        (0.86, false),
        (0.99, false),
        (1.0, true),
        (0.92, true),
    ] {
        weak_view
            .set_viewport(
                ViewportTransform::new(CanvasPoint::default(), zoom)
                    .expect("valid hysteresis viewport transform"),
            )
            .unwrap();
        draw(cx);
        assert_eq!(
            cx.debug_bounds(FIXED_NATIVE_CONTROL_SELECTOR).is_some(),
            expects_detail,
            "unexpected detail state at zoom {zoom}",
        );
        assert_eq!(
            cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some(),
            !expects_detail,
            "unexpected overview state at zoom {zoom}",
        );
    }

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1)
    });
}

#[open_gpui::test]
fn protected_node_uses_graph_inert_interaction_island_at_low_zoom_without_remounting(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 0.5)
            .expect("valid interaction-island viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();
    let renders = Rc::new(Cell::new(0));
    let factory_renders = renders.clone();
    let lease = Rc::new(RefCell::new(None::<OpenGpuiActivityLease>));
    let factory_lease = lease.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let focus = binding
            .read(cx)
            .activity()
            .acquire(OpenGpuiActivityKind::Focus)
            .expect("focus lease");
        *factory_lease.borrow_mut() = Some(focus);
        cx.new(|_| FixedIntrinsicChildNode {
            binding,
            renders: factory_renders.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let native_control = cx
        .debug_bounds(FIXED_NATIVE_CONTROL_SELECTOR)
        .expect("protected node keeps its native child");
    assert_close(native_control.size.width.as_f32(), 220.0);
    assert_close(native_control.size.height.as_f32(), 120.0);
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some());
    let protected_geometry = weak_view.geometry_snapshot().unwrap();
    let protected_node = protected_geometry.node(NodeId::from_u128(10)).unwrap();
    let protected_size = protected_node.bounds().size;
    assert_close_with(
        protected_size.width,
        220.0 / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
        1.1,
    );
    assert_close_with(
        protected_size.height,
        120.0 / OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
        1.1,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });

    let renders_before_release = renders.get();
    drop(lease.borrow_mut().take());
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    assert!(cx.debug_bounds(FIXED_NATIVE_CONTROL_SELECTOR).is_none());
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some());
    assert_eq!(renders.get(), renders_before_release);
    let overview_geometry = weak_view.geometry_snapshot().unwrap();
    assert_eq!(
        overview_geometry
            .node(NodeId::from_u128(10))
            .unwrap()
            .bounds()
            .size,
        protected_size,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 1);
    });
}

#[open_gpui::test]
fn graph_interaction_protects_retention_without_creating_an_interaction_island(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 0.5)
            .expect("valid graph-interaction viewport transform"),
    )
    .unwrap();
    let renders = Rc::new(Cell::new(0));
    let factory_renders = renders.clone();
    let lease = Rc::new(RefCell::new(None::<OpenGpuiActivityLease>));
    let factory_lease = lease.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        *factory_lease.borrow_mut() = Some(
            binding
                .read(cx)
                .activity()
                .acquire(OpenGpuiActivityKind::GraphInteraction)
                .expect("graph-interaction lease"),
        );
        cx.new(|_| FixedIntrinsicChildNode {
            binding,
            renders: factory_renders.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    assert!(cx.debug_bounds(FIXED_NATIVE_CONTROL_SELECTOR).is_none());
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
    });
    assert_eq!(renders.get(), 0);
    drop(lease.borrow_mut().take());
}

#[open_gpui::test]
fn late_incomplete_feedback_after_runtime_drop_is_silently_discarded(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let runtime_owner = Rc::new(RefCell::new(Some(runtime)));
    let factory_runtime_owner = runtime_owner.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RuntimeDroppingIncompleteNode {
            binding,
            runtime_owner: factory_runtime_owner.clone(),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(host.last_diagnostic().is_none());
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn assert_close(actual: f32, expected: f32) {
    assert_close_with(actual, expected, 1.0e-4);
}

fn assert_close_with(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {actual} to be within {tolerance} of {expected}",
    );
}

fn assert_hit_contains_visual(hit: CanvasRect, visual: CanvasRect) {
    assert!(hit.origin.x <= visual.origin.x);
    assert!(hit.origin.y <= visual.origin.y);
    assert!(hit.origin.x + hit.size.width >= visual.origin.x + visual.size.width);
    assert!(hit.origin.y + hit.size.height >= visual.origin.y + visual.size.height);
}

fn graph() -> jellyflow_core::Graph {
    let node_id = NodeId::from_u128(10);
    let node = Node {
        kind: NodeKindKey::new(KIND),
        kind_version: 1,
        pos: CanvasPoint { x: 20.0, y: 20.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 220.0,
            height: 120.0,
        }),
        hidden: false,
        collapsed: false,
        ports: vec![input_id(), output_id()],
        data: serde_json::json!({ "label": "measured" }),
    };
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node_id, node);
    graph.insert_port(input_id(), port(node_id, "input", PortDirection::In));
    graph.insert_port(output_id(), port(node_id, "output", PortDirection::Out));
    graph.build().unwrap()
}

fn graph_with_valid_sibling() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::from_detached_graph(graph());
    graph.insert_node(
        NodeId::from_u128(11),
        Node {
            kind: NodeKindKey::new(VALID_SIBLING_KIND),
            kind_version: 1,
            pos: CanvasPoint { x: 300.0, y: 20.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 140.0,
                height: 80.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        },
    );
    graph.build().unwrap()
}

fn graph_with_route_and_valid_sibling() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::from_detached_graph(graph_with_valid_sibling());
    graph.insert_edge(
        edge_id(),
        Edge::new(EdgeKind::Data, output_id(), input_id()),
    );
    graph.build().expect("valid routed sibling graph")
}

fn graph_with_content_sibling() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::from_detached_graph(content_graph());
    graph.insert_node(
        NodeId::from_u128(11),
        Node {
            kind: NodeKindKey::new(KIND),
            kind_version: 1,
            pos: CanvasPoint { x: 300.0, y: 20.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 220.0,
                height: 120.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::json!({ "label": "content sibling" }),
        },
    );
    graph.build().unwrap()
}

fn content_graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::from_detached_graph(graph());
    graph.remove_port(&input_id());
    graph.remove_port(&output_id());
    graph
        .node_mut(&NodeId::from_u128(10))
        .expect("content node")
        .ports
        .clear();
    graph.build().expect("valid portless content graph")
}

fn port(node: NodeId, key: &str, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn input_id() -> PortId {
    PortId::from_u128(20)
}

fn output_id() -> PortId {
    PortId::from_u128(21)
}

fn edge_id() -> EdgeId {
    EdgeId::from_u128(30)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Measured node", RENDERER)
                .default_size(CanvasSize {
                    width: 220.0,
                    height: 120.0,
                })
                .build(),
        )
        .unwrap();
    catalog
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}
