use std::cell::Cell;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{
    GraphCommand, GraphRuntime, ViewCloseReason, ViewCloseRequest, ViewSessionError,
    ViewSessionOptions,
};
use jellyflow_open_gpui::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeBinding, OpenGpuiNodeMountError,
    OpenGpuiNodeRegistry, OpenGpuiSceneHost, OpenGpuiSceneHostCloseError,
};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, VisualContext, Window, div};

const KIND: &str = "test.workflow-node";
const RENDERER: &str = "test.native.workflow-node";

struct RetainedProbeNode {
    _binding: Entity<OpenGpuiNodeBinding>,
    renders: Rc<Cell<u32>>,
}

struct TerminalProbeNode {
    _binding: Entity<OpenGpuiNodeBinding>,
    _activity: OpenGpuiActivityLease,
}

impl Render for RetainedProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.renders.set(self.renders.get().saturating_add(1));
        div().child("retained workflow node")
    }
}

impl Render for TerminalProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("terminal lifecycle node")
    }
}

#[open_gpui::test]
fn scene_host_parks_and_restores_the_same_retained_node(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mounts = Rc::new(Cell::new(0_u32));
    let renders = Rc::new(Cell::new(0_u32));
    let factory_mounts = mounts.clone();
    let factory_renders = renders.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        factory_mounts.set(factory_mounts.get().saturating_add(1));
        cx.new(|_| RetainedProbeNode {
            _binding: binding,
            renders: factory_renders.clone(),
        })
    });
    let registry = registry.seal().expect("valid test registry");

    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);
    assert_eq!(mounts.get(), 1);
    assert!(renders.get() > 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 0);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    assert_eq!(mounts.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(visible_viewport());
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        mounts.get(),
        1,
        "parked node must reuse its retained entity"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().mounts(), 1);
        assert!(host.stats().restores() >= 1);
    });
}

#[open_gpui::test]
fn factory_failure_recovers_only_after_an_explicit_mount_retry(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let ready = Rc::new(Cell::new(false));
    let mount_attempts = Rc::new(Cell::new(0_u32));
    let renders = Rc::new(Cell::new(0_u32));
    let factory_ready = ready.clone();
    let factory_attempts = mount_attempts.clone();
    let factory_renders = renders.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        if !factory_ready.get() {
            return Err(OpenGpuiNodeMountError::new("dependency unavailable"));
        }
        Ok(cx.new(|_| RetainedProbeNode {
            _binding: binding,
            renders: factory_renders.clone(),
        }))
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    assert_eq!(mount_attempts.get(), 1);
    assert_eq!(renders.get(), 0);
    let retry = cx
        .debug_bounds(&factory_retry_selector(NodeId::from_u128(10)))
        .expect("factory failure exposes a native retry control");

    ready.set(true);
    draw(cx);
    assert_eq!(
        mount_attempts.get(),
        1,
        "a retained fallback must not retry the factory implicitly"
    );

    cx.simulate_click(retry.center(), Default::default());
    draw(cx);

    assert_eq!(mount_attempts.get(), 2);
    assert!(renders.get() > 0);
    assert!(
        cx.debug_bounds(&factory_retry_selector(NodeId::from_u128(10)))
            .is_none(),
        "a successful matching mount retires its factory fallback"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().hard_retirements(), 1);
        assert!(
            host.last_diagnostic().is_none(),
            "the matching factory diagnostic clears only after a successful remount"
        );
    });
    assert!(!cx.update_entity(&host, |host, cx| {
        host.retry_mount(NodeId::from_u128(10), cx)
    }));
}

#[open_gpui::test]
fn missing_renderer_fallback_is_not_mount_retryable(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let registry = OpenGpuiNodeRegistry::builder()
        .seal()
        .expect("valid empty test registry");
    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);

    assert!(
        cx.debug_bounds(&factory_retry_selector(NodeId::from_u128(10)))
            .is_none(),
        "a missing renderer is configuration failure, not a retryable factory failure"
    );

    assert!(!cx.update_entity(&host, |host, cx| {
        host.retry_mount(NodeId::from_u128(10), cx)
    }));
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().mounts(), 1);
        assert_eq!(host.stats().hard_retirements(), 0);
    });
}

#[open_gpui::test]
fn explicit_close_drains_final_command_once_then_retires_native_state(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(Cell::new(0_u32));
    let factory_revocations = revocations.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let activity = binding.read(cx).activity().clone();
        let revocations = factory_revocations.clone();
        let lease = activity
            .acquire_with_revocation(OpenGpuiActivityKind::Task, move || {
                revocations.set(revocations.get().saturating_add(1));
            })
            .unwrap();
        cx.new(|_| TerminalProbeNode {
            _binding: binding,
            _activity: lease,
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);

    let outcome = cx
        .update_window_entity(&host, |host, window, cx| {
            host.close_with_request_in_window(
                ViewCloseRequest::new().with_command(GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prompt": "saved on close" }),
                )),
                window,
                cx,
            )
        })
        .unwrap();

    assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
    assert_eq!(outcome.receipts().len(), 1);
    assert!(outcome.receipts()[0].patch().is_some());
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(revocations.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().hard_retirements(), 1);
    });

    assert!(matches!(
        cx.update_entity(&host, |host, _| host.close()),
        Err(OpenGpuiSceneHostCloseError::Session(
            ViewSessionError::Closed {
                reason: ViewCloseReason::HostClosed,
            }
        )),
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(revocations.get(), 1);
}

#[open_gpui::test]
fn runtime_drop_retires_native_state_on_the_next_host_tick(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RetainedProbeNode {
            _binding: binding,
            renders: Rc::new(Cell::new(0)),
        })
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
    });

    drop(runtime);
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
        assert_eq!(host.stats().hard_retirements(), 1);
    });
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn factory_retry_selector(node: NodeId) -> String {
    format!("jellyflow-factory-retry-{}", node.0)
}

fn graph() -> jellyflow::core::Graph {
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
        ports: Vec::new(),
        data: serde_json::json!({ "prompt": "hello" }),
    };
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(NodeId::from_u128(10), node);
    graph.build().unwrap()
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Workflow node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
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
