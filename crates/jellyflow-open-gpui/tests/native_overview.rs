use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{OpenGpuiSceneHost, OpenGpuiSceneHostDiagnosticKind};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeMountError, OpenGpuiNodeOverviewMount,
    OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    graph::GraphRuntime, view::NodeInstanceKey, view::RendererStamp, view::ViewSessionOptions,
    viewport::ViewportTransform,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render, Styled, Window, div, px,
};

const KIND: &str = "test.native-overview";
const RENDERER: &str = "test.native-overview.renderer";
const DETAIL_SELECTOR: &str = "jellyflow-test-overview-detail";
const NATIVE_OVERVIEW_SELECTOR: &str = "jellyflow-test-native-overview";
const GENERIC_OVERVIEW_SELECTOR: &str = "jellyflow-overview-node";

struct DetailNode {
    binding: Entity<OpenGpuiNodeBinding>,
    renders: Rc<Cell<u32>>,
}

struct NativeOverviewNode {
    mount: OpenGpuiNodeOverviewMount,
    renders: Rc<Cell<u32>>,
}

impl Render for DetailNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.renders.set(self.renders.get().saturating_add(1));
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("detail binding remains live");
        let input = authoring
            .port(input_id())
            .expect("detail input")
            .handle()
            .clone();
        let output = authoring
            .port(output_id())
            .expect("detail output")
            .handle()
            .clone();

        port_surface(DETAIL_SELECTOR, input, output)
    }
}

impl Render for NativeOverviewNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.renders.set(self.renders.get().saturating_add(1));
        let binding = self.mount.binding();
        let authoring = binding
            .read(cx)
            .authoring()
            .expect("overview binding remains live");
        assert_eq!(authoring.instance(), self.mount.facts().instance());
        let input = self
            .mount
            .port_handle(input_id(), cx)
            .expect("overview input capability");
        let output = self
            .mount
            .port_handle(output_id(), cx)
            .expect("overview output capability");
        assert_eq!(input.instance(), self.mount.facts().instance());
        assert_eq!(output.instance(), self.mount.facts().instance());

        port_surface(NATIVE_OVERVIEW_SELECTOR, input, output)
    }
}

fn port_surface(
    selector: &'static str,
    input: jellyflow_open_gpui::authoring::OpenGpuiPortHandle,
    output: jellyflow_open_gpui::authoring::OpenGpuiPortHandle,
) -> impl open_gpui::IntoElement {
    div()
        .debug_selector(move || selector.to_owned())
        .relative()
        .size_full()
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(43.0))
                .child(input.measure(div().w(px(14.0)).h(px(14.0)))),
        )
        .child(
            div()
                .absolute()
                .right(px(0.0))
                .top(px(43.0))
                .child(output.measure(div().w(px(14.0)).h(px(14.0)))),
        )
}

#[open_gpui::test]
fn application_overview_reuses_one_mount_and_publishes_real_ports(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 0.5)
            .expect("valid native-overview viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();
    let detail_mounts = Rc::new(Cell::new(0_u32));
    let detail_renders = Rc::new(Cell::new(0_u32));
    let overview_mounts = Rc::new(Cell::new(0_u32));
    let overview_renders = Rc::new(Cell::new(0_u32));
    let mounted_instance = Rc::new(RefCell::new(None::<NodeInstanceKey>));

    let mut registry = OpenGpuiNodeRegistry::builder();
    let factory_detail_mounts = Rc::clone(&detail_mounts);
    let factory_detail_renders = Rc::clone(&detail_renders);
    let factory_overview_mounts = Rc::clone(&overview_mounts);
    let factory_overview_renders = Rc::clone(&overview_renders);
    let factory_mounted_instance = Rc::clone(&mounted_instance);
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                KIND,
                1,
                RendererStamp::new(RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Native overview"),
                move |binding, _window, cx| {
                    factory_detail_mounts.set(factory_detail_mounts.get().saturating_add(1));
                    Ok(cx.new(|_| DetailNode {
                        binding,
                        renders: Rc::clone(&factory_detail_renders),
                    }))
                },
            )
            .with_overview(move |mount, _window, cx| {
                factory_overview_mounts.set(factory_overview_mounts.get().saturating_add(1));
                *factory_mounted_instance.borrow_mut() = Some(mount.facts().instance().clone());
                Ok(cx.new(|_| NativeOverviewNode {
                    mount,
                    renders: Rc::clone(&factory_overview_renders),
                }))
            }),
        )
        .expect("unique native-overview renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&node_catalog)
                .expect("valid native overview registry"),
            visible_viewport(),
        )
        .expect("valid Open GPUI scene host")
    });

    draw(cx);
    assert!(cx.debug_bounds(NATIVE_OVERVIEW_SELECTOR).is_some());
    assert!(cx.debug_bounds(DETAIL_SELECTOR).is_none());
    assert_eq!(detail_mounts.get(), 1);
    assert_eq!(overview_mounts.get(), 1);
    assert!(overview_renders.get() > 0);
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.port(input_id()).is_some());
    assert!(geometry.port(output_id()).is_some());
    let published_node = geometry.node(NodeId::from_u128(10)).unwrap();
    let mounted_instance = mounted_instance.borrow();
    let mounted_instance = mounted_instance.as_ref().unwrap();
    assert_eq!(published_node.lifetime(), mounted_instance.lifetime());
    assert_eq!(published_node.renderer(), mounted_instance.renderer());

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 1.0)
                .expect("valid native-overview detail viewport transform"),
        )
        .unwrap();
    draw(cx);
    assert!(cx.debug_bounds(DETAIL_SELECTOR).is_some());
    assert!(cx.debug_bounds(NATIVE_OVERVIEW_SELECTOR).is_none());
    assert!(detail_renders.get() > 0);

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 0.5)
                .expect("valid native-overview overview viewport transform"),
        )
        .unwrap();
    draw(cx);
    assert!(cx.debug_bounds(NATIVE_OVERVIEW_SELECTOR).is_some());
    assert_eq!(
        overview_mounts.get(),
        1,
        "zooming must not remount overview"
    );

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().parked_nodes(), 1)
    });
    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(visible_viewport());
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        overview_mounts.get(),
        1,
        "restore must retain overview entity"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1)
    });
}

#[open_gpui::test]
fn overview_factory_error_keeps_detail_and_uses_generic_overview(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 0.5)
            .expect("valid native-overview fallback viewport transform"),
    )
    .unwrap();
    let weak_view = view.downgrade();
    let detail_renders = Rc::new(Cell::new(0_u32));
    let factory_detail_renders = Rc::clone(&detail_renders);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                KIND,
                1,
                RendererStamp::new(RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Native overview"),
                move |binding, _window, cx| {
                    Ok(cx.new(|_| DetailNode {
                        binding,
                        renders: Rc::clone(&factory_detail_renders),
                    }))
                },
            )
            .with_overview(
                |_mount, _window, _cx| -> Result<Entity<NativeOverviewNode>, _> {
                    Err(OpenGpuiNodeMountError::new(
                        "overview dependency unavailable",
                    ))
                },
            ),
        )
        .expect("unique fallible native-overview renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&node_catalog)
                .expect("valid fallible overview registry"),
            visible_viewport(),
        )
        .expect("valid Open GPUI scene host")
    });

    draw(cx);
    assert!(cx.debug_bounds(GENERIC_OVERVIEW_SELECTOR).is_some());
    assert!(cx.debug_bounds(NATIVE_OVERVIEW_SELECTOR).is_none());
    assert!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(input_id())
            .is_some()
    );
    cx.read_entity(&host, |host, _| {
        let diagnostic = host.last_diagnostic().expect("overview failure diagnostic");
        assert_eq!(
            diagnostic.kind(),
            OpenGpuiSceneHostDiagnosticKind::FactoryFailure
        );
        assert!(
            diagnostic
                .message()
                .contains("overview dependency unavailable")
        );
    });
    assert!(!cx.update_entity(&host, |host, cx| {
        host.retry_mount(NodeId::from_u128(10), cx)
    }));

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 1.0)
                .expect("valid native-overview fallback detail viewport transform"),
        )
        .unwrap();
    draw(cx);
    assert!(cx.debug_bounds(DETAIL_SELECTOR).is_some());
    assert!(detail_renders.get() > 0);
}

#[open_gpui::test]
fn panicking_overview_factory_isolated_to_generic_overview(cx: &mut open_gpui::TestAppContext) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_viewport(
        ViewportTransform::new(CanvasPoint::default(), 0.5)
            .expect("valid native-overview panic viewport transform"),
    )
    .unwrap();
    let detail_renders = Rc::new(Cell::new(0_u32));
    let factory_detail_renders = Rc::clone(&detail_renders);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                KIND,
                1,
                RendererStamp::new(RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Native overview"),
                move |binding, _window, cx| {
                    Ok(cx.new(|_| DetailNode {
                        binding,
                        renders: Rc::clone(&factory_detail_renders),
                    }))
                },
            )
            .with_overview(
                |_mount,
                 _window,
                 _cx|
                 -> Result<Entity<NativeOverviewNode>, OpenGpuiNodeMountError> {
                    panic!("injected native overview mount panic")
                },
            ),
        )
        .expect("unique panicking native-overview renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&node_catalog)
                .expect("valid panicking overview registry"),
            visible_viewport(),
        )
        .expect("valid Open GPUI scene host")
    });

    draw(cx);
    assert!(cx.debug_bounds(GENERIC_OVERVIEW_SELECTOR).is_some());
    assert!(cx.debug_bounds(NATIVE_OVERVIEW_SELECTOR).is_none());
    cx.read_entity(&host, |host, _| {
        assert!(
            host.last_diagnostic().is_some_and(|diagnostic| diagnostic
                .message()
                .contains("overview factory panicked"))
        );
    });
    assert_eq!(detail_renders.get(), 0, "low zoom does not paint detail");
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn graph() -> jellyflow_core::Graph {
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
            width: 180.0,
            height: 100.0,
        }),
        hidden: false,
        collapsed: false,
        ports: vec![input_id(), output_id()],
        data: serde_json::json!({ "label": "native overview" }),
    };
    let node_id = NodeId::from_u128(10);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node_id, node);
    graph.insert_port(input_id(), port(node_id, "input", PortDirection::In));
    graph.insert_port(output_id(), port(node_id, "output", PortDirection::Out));
    graph.build().unwrap()
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

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .unwrap();
    catalog
}

fn visible_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}

fn offscreen_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: 10_000.0,
            y: 10_000.0,
        },
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}
