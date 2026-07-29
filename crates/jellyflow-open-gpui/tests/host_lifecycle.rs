use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_open_gpui::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OpenGpuiSceneHost, OpenGpuiSceneHostCloseError,
};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeMountError, OpenGpuiNodeRegistry,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
    graph::GraphRuntime, view::RendererStamp, view::ViewCloseReason, view::ViewCloseRequest,
    view::ViewSessionError, view::ViewSessionOptions,
};
use open_gpui::{
    App, AppContext, Context, Entity, ParentElement, Render, VisualContext, Window, div,
};

const KIND: &str = "test.workflow-node";
const RENDERER: &str = "test.native.workflow-node";

fn renderer_descriptor<F, V>(factory: F) -> OpenGpuiNodeRendererDescriptor
where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    fallible_renderer_descriptor(move |binding, window, cx| Ok(factory(binding, window, cx)))
}

fn fallible_renderer_descriptor<F, V>(factory: F) -> OpenGpuiNodeRendererDescriptor
where
    F: Fn(
            Entity<OpenGpuiNodeBinding>,
            &mut Window,
            &mut App,
        ) -> Result<Entity<V>, OpenGpuiNodeMountError>
        + 'static,
    V: Render + 'static,
{
    OpenGpuiNodeRendererDescriptor::new(
        KIND,
        1,
        RendererStamp::new(RENDERER, 1),
        OpenGpuiNodeAdapterPresentation::new("Workflow node"),
        factory,
    )
}

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
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mounts = Rc::new(Cell::new(0_u32));
    let renders = Rc::new(Cell::new(0_u32));
    let factory_mounts = mounts.clone();
    let factory_renders = renders.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
            factory_mounts.set(factory_mounts.get().saturating_add(1));
            cx.new(|_| RetainedProbeNode {
                _binding: binding,
                renders: factory_renders.clone(),
            })
        }))
        .expect("unique workflow-node renderer");
    let registry = registry.seal(&node_catalog).expect("valid test registry");

    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);
    assert_eq!(mounts.get(), 1);
    assert!(renders.get() > 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    assert_eq!(mounts.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
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
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
    });
}

#[open_gpui::test]
fn factory_failure_recovers_only_after_an_explicit_mount_retry(cx: &mut open_gpui::TestAppContext) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let ready = Rc::new(Cell::new(false));
    let mount_attempts = Rc::new(Cell::new(0_u32));
    let renders = Rc::new(Cell::new(0_u32));
    let factory_ready = ready.clone();
    let factory_attempts = mount_attempts.clone();
    let factory_renders = renders.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(fallible_renderer_descriptor(move |binding, _window, cx| {
            factory_attempts.set(factory_attempts.get().saturating_add(1));
            if !factory_ready.get() {
                return Err(OpenGpuiNodeMountError::new("dependency unavailable"));
            }
            Ok(cx.new(|_| RetainedProbeNode {
                _binding: binding,
                renders: factory_renders.clone(),
            }))
        }))
        .expect("unique workflow-node renderer");

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
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
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
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
fn panicking_factory_fallback_retirement_is_diagnosed_and_the_retry_continues(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let ready = Rc::new(Cell::new(false));
    let mount_attempts = Rc::new(Cell::new(0_u32));
    let leases = Rc::new(RefCell::new(Vec::new()));
    let factory_ready = Rc::clone(&ready);
    let factory_attempts = Rc::clone(&mount_attempts);
    let factory_leases = Rc::clone(&leases);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(fallible_renderer_descriptor(move |binding, _window, cx| {
            factory_attempts.set(factory_attempts.get().saturating_add(1));
            if !factory_ready.get() {
                let lease = binding
                    .read(cx)
                    .activity()
                    .acquire_with_revocation(OpenGpuiActivityKind::Task, || {
                        panic!("injected factory fallback retirement panic");
                    })
                    .expect("fallback retirement activity");
                factory_leases.borrow_mut().push(lease);
                return Err(OpenGpuiNodeMountError::new("dependency unavailable"));
            }
            Ok(cx.new(|_| RetainedProbeNode {
                _binding: binding,
                renders: Rc::new(Cell::new(0)),
            }))
        }))
        .expect("unique workflow-node renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    assert_eq!(mount_attempts.get(), 1);

    ready.set(true);
    assert!(cx.update_entity(&host, |host, cx| {
        host.retry_mount(NodeId::from_u128(10), cx)
    }));

    cx.run_until_parked();
    assert_eq!(
        mount_attempts.get(),
        2,
        "retry notification must schedule the replacement mount before propagating the panic",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(
            host.last_diagnostic()
                .is_some_and(|diagnostic| diagnostic.message().contains("retry continued"))
        );
    });
    drop(leases);
}

#[open_gpui::test]
fn missing_renderer_fallback_is_not_mount_retryable(cx: &mut open_gpui::TestAppContext) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let registry = OpenGpuiNodeRegistry::builder()
        .seal(&node_catalog)
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
        assert_eq!(host.resource_snapshot().active_nodes(), 1)
    });
}

#[open_gpui::test]
fn explicit_close_drains_final_command_once_then_retires_native_state(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(Cell::new(0_u32));
    let factory_revocations = revocations.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
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
        }))
        .expect("unique workflow-node renderer");

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    let close_version = runtime.snapshot().expect("close snapshot").version();

    let outcome = cx
        .update_window_entity(&host, |host, window, cx| {
            host.close_with_request_in_window(
                ViewCloseRequest::new().with_command(GraphCommand::set_node_data(
                    close_version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prompt": "saved on close" }),
                )),
                window,
                cx,
            )
        })
        .unwrap();

    assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
    assert_eq!(outcome.command_completions().count(), 1);
    assert!(
        outcome
            .command_completions()
            .next()
            .expect("close command completion")
            .result()
            .expect("close command executes")
            .applied()
            .is_some()
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(revocations.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
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
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
            cx.new(|_| RetainedProbeNode {
                _binding: binding,
                renders: Rc::new(Cell::new(0)),
            })
        }))
        .expect("unique workflow-node renderer");
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
    });

    drop(runtime);
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn runtime_drop_bridge_retires_every_mount_after_the_first_revocation_panics(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph_with_node_ids([10, 20]))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(RefCell::new(Vec::new()));
    let registry = panicking_retirement_registry(
        &node_catalog,
        Rc::clone(&revocations),
        NodeId::from_u128(10),
    );
    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);

    drop(runtime);
    cx.run_until_parked();

    assert_eq!(
        &*revocations.borrow(),
        &[NodeId::from_u128(10), NodeId::from_u128(20)],
        "a terminal bridge must retire every detached mount before exposing a callback panic",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
        assert!(
            host.last_diagnostic()
                .is_some_and(|diagnostic| diagnostic.message().contains("completed cleanup"))
        );
    });

    draw(cx);
}

#[open_gpui::test]
fn nonterminal_retirement_panic_does_not_stop_the_lifecycle_bridge(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph_with_node_ids([10, 20]))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(RefCell::new(Vec::new()));
    let registry = panicking_retirement_registry(
        &node_catalog,
        Rc::clone(&revocations),
        NodeId::from_u128(10),
    );
    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("first retirement snapshot")
                .version(),
            TopologyEdit::new().remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();
    assert_eq!(&*revocations.borrow(), &[NodeId::from_u128(10)]);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(
            host.last_diagnostic()
                .is_some_and(|diagnostic| diagnostic.message().contains("bridge continued"))
        );
    });

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("second retirement snapshot")
                .version(),
            TopologyEdit::new().remove_node(NodeId::from_u128(20), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();

    assert_eq!(
        &*revocations.borrow(),
        &[NodeId::from_u128(10), NodeId::from_u128(20)],
        "the bridge must consume later patches after isolating a callback panic",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn stale_parked_reconciliation_finishes_every_retirement_before_propagating_a_panic(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph_with_node_ids([10, 20]))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(RefCell::new(Vec::new()));
    let registry = panicking_retirement_registry(
        &node_catalog,
        Rc::clone(&revocations),
        NodeId::from_u128(10),
    );
    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);
    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().parked_nodes(), 2);
    });

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("parked retirement snapshot")
                .version(),
            TopologyEdit::new()
                .remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject)
                .remove_node(NodeId::from_u128(20), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    let panic = catch_unwind(AssertUnwindSafe(|| draw_once(cx)));

    assert!(
        panic.is_err(),
        "the first callback panic remains observable"
    );
    assert_eq!(
        &*revocations.borrow(),
        &[NodeId::from_u128(10), NodeId::from_u128(20)],
        "stale parked mounts must all retire before the first panic is propagated",
    );
}

#[open_gpui::test]
fn unseen_eviction_retires_every_overflow_mount_before_propagating_a_panic(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_count = OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY + 2;
    let node_catalog = catalog();
    let runtime = GraphRuntime::builder(node_catalog.clone())
        .build(graph_with_node_ids(
            (1..=node_count).map(|node| u128::try_from(node).unwrap()),
        ))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(RefCell::new(Vec::new()));
    let registry =
        panicking_retirement_registry(&node_catalog, Rc::clone(&revocations), NodeId::from_u128(1));
    let (host, cx) =
        cx.add_window_view(move |_, _| OpenGpuiSceneHost::new(view, registry, visible_viewport()));
    draw(cx);

    cx.update_entity(&host, |host, _| {
        host.set_frame_bounds(offscreen_viewport());
    });
    let panic = catch_unwind(AssertUnwindSafe(|| draw_once(cx)));

    assert!(
        panic.is_err(),
        "the first callback panic remains observable"
    );
    let expected = (1..=node_count)
        .map(|node| NodeId::from_u128(u128::try_from(node).unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        &*revocations.borrow(),
        &expected,
        "the evicted prefix and every mount released by host unwind must retire in order",
    );
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    draw_once(cx);
    cx.run_until_parked();
}

fn draw_once(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
}

fn panicking_retirement_registry(
    catalog: &NodeCatalog,
    revocations: Rc<RefCell<Vec<NodeId>>>,
    panic_node: NodeId,
) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
            let node = binding
                .read(cx)
                .authoring()
                .expect("live test binding")
                .node_id();
            let callback_revocations = Rc::clone(&revocations);
            let activity = binding
                .read(cx)
                .activity()
                .acquire_with_revocation(OpenGpuiActivityKind::Task, move || {
                    callback_revocations.borrow_mut().push(node);
                    assert_ne!(node, panic_node, "injected mount revocation panic");
                })
                .unwrap();
            cx.new(|_| TerminalProbeNode {
                _binding: binding,
                _activity: activity,
            })
        }))
        .expect("unique workflow-node renderer");
    registry
        .seal(catalog)
        .expect("valid retirement test registry")
}

fn factory_retry_selector(node: NodeId) -> String {
    format!("jellyflow-factory-retry-{}", node.0)
}

fn graph() -> jellyflow_core::Graph {
    graph_with_node_ids([10])
}

fn graph_with_node_ids(node_ids: impl IntoIterator<Item = u128>) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    for node in node_ids {
        graph.insert_node(NodeId::from_u128(node), test_node());
    }
    graph.build().unwrap()
}

fn test_node() -> Node {
    Node {
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
    }
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
