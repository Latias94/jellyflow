use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{GraphCommand, GraphRuntime, ViewSessionOptions};
use jellyflow_open_gpui::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OpenGpuiConnectionPhase, OpenGpuiNodeBinding,
    OpenGpuiNodeBindingRead, OpenGpuiNodeBindingReadError, OpenGpuiNodeMountError,
    OpenGpuiNodeRegistry, OpenGpuiPortDragPhase, OpenGpuiPortEvent, OpenGpuiPortHandle,
    OpenGpuiSceneHost,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Render, Styled, Window, div, px,
};

const KIND: &str = "test.mount-capability-node";
const RENDERER: &str = "test.native.mount-capability-node";

struct ProbeNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

struct InteractiveProbeNode {
    binding: Entity<OpenGpuiNodeBinding>,
    phases: Rc<RefCell<Vec<OpenGpuiPortDragPhase>>>,
}

struct ExternalPortScene {
    host: Entity<OpenGpuiSceneHost>,
    stale_handle: Rc<RefCell<Option<OpenGpuiPortHandle>>>,
    callbacks: Rc<Cell<u32>>,
}

impl Render for ProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("mount capability probe")
    }
}

impl Render for InteractiveProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let handle = self.binding.read(cx).port_handle(port_id());
        let phases = self.phases.clone();
        div().relative().size_full().children(handle.map(|handle| {
            div()
                .absolute()
                .right(px(8.0))
                .top(px(40.0))
                .child(
                    handle.interactive(
                        div()
                            .debug_selector(|| "live-mount-port".to_owned())
                            .w(px(16.0))
                            .h(px(16.0)),
                        move |event, _, _| {
                            if let OpenGpuiPortEvent::Pointer(event) = event {
                                phases.borrow_mut().push(event.phase());
                            }
                        },
                    ),
                )
                .into_any_element()
        }))
    }
}

impl Render for ExternalPortScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let callbacks = self.callbacks.clone();
        let stale_port = self.stale_handle.borrow().clone().map(|handle| {
            handle
                .interactive(
                    div()
                        .debug_selector(|| "stale-mount-port".to_owned())
                        .w(px(24.0))
                        .h(px(24.0)),
                    move |_, _, _| callbacks.set(callbacks.get().saturating_add(1)),
                )
                .into_any_element()
        });
        div().relative().size_full().child(self.host.clone()).child(
            div()
                .absolute()
                .right(px(20.0))
                .top(px(20.0))
                .children(stale_port),
        )
    }
}

#[open_gpui::test]
fn eligible_lru_eviction_revokes_old_binding_before_same_instance_remount(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_count = OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY + 2;
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph_with_nodes(node_count))
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let bindings = Rc::new(RefCell::new(BTreeMap::<
        NodeId,
        Vec<Entity<OpenGpuiNodeBinding>>,
    >::new()));
    let factory_bindings = bindings.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = binding.read(cx).snapshot().node_id();
        factory_bindings
            .borrow_mut()
            .entry(node)
            .or_default()
            .push(binding.clone());
        cx.new(|_| ProbeNode { _binding: binding })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);

    let old = bindings.borrow()[&node_id()][0].clone();
    let old_instance = cx.read_entity(&old, |binding, _| binding.snapshot().instance().clone());
    let stale_execute = binding_read(cx, &old);
    let stale_handles = binding_read(cx, &old);
    cx.read_entity(&old, |binding, _| {
        assert!(binding.port_handle(port_id()).is_some());
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    assert_eq!(bindings.borrow()[&node_id()].len(), 1);
    assert_revoked_binding(cx, &old);

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(visible_viewport());
        cx.notify();
    });
    draw(cx);

    let new = bindings.borrow()[&node_id()][1].clone();
    let new_instance = cx.read_entity(&new, |binding, _| binding.snapshot().instance().clone());
    assert_eq!(
        old_instance, new_instance,
        "runtime identity survives eviction"
    );
    assert_revoked_binding(cx, &old);
    assert_stale_reads_rejected(stale_execute, stale_handles, &old, cx);
    assert_live_replacement(&runtime, cx, &new, serde_json::json!("after eviction"));
}

#[open_gpui::test]
fn retry_mount_revokes_failed_factory_binding_before_same_instance_remount(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph_with_nodes(1))
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let attempts = Rc::new(Cell::new(0_u32));
    let bindings = Rc::new(RefCell::new(Vec::<Entity<OpenGpuiNodeBinding>>::new()));
    let factory_attempts = attempts.clone();
    let factory_bindings = bindings.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        factory_bindings.borrow_mut().push(binding.clone());
        if factory_attempts.get() == 1 {
            return Err(OpenGpuiNodeMountError::new("retry fixture"));
        }
        Ok(cx.new(|_| ProbeNode { _binding: binding }))
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);

    let old = bindings.borrow()[0].clone();
    let old_instance = cx.read_entity(&old, |binding, _| binding.snapshot().instance().clone());
    let stale_execute = binding_read(cx, &old);
    let stale_handles = binding_read(cx, &old);
    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));
    assert_revoked_binding(cx, &old);

    draw(cx);
    assert_eq!(attempts.get(), 2);
    let new = bindings.borrow()[1].clone();
    let new_instance = cx.read_entity(&new, |binding, _| binding.snapshot().instance().clone());
    assert_eq!(old_instance, new_instance, "retry reuses runtime identity");
    assert_revoked_binding(cx, &old);
    assert_stale_reads_rejected(stale_execute, stale_handles, &old, cx);
    assert_live_replacement(&runtime, cx, &new, serde_json::json!("after retry"));
}

#[open_gpui::test]
fn old_port_handle_cannot_dispatch_after_retry_or_same_instance_remount(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(1))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let attempts = Rc::new(Cell::new(0_u32));
    let stale_handle = Rc::new(RefCell::new(None::<OpenGpuiPortHandle>));
    let callbacks = Rc::new(Cell::new(0_u32));
    let factory_attempts = attempts.clone();
    let factory_handle = stale_handle.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        if factory_handle.borrow().is_none() {
            *factory_handle.borrow_mut() = binding.read(cx).port_handle(port_id());
        }
        if factory_attempts.get() == 1 {
            return Err(OpenGpuiNodeMountError::new("retry interaction fixture"));
        }
        Ok(cx.new(|_| ProbeNode { _binding: binding }))
    });

    let captured_host = Rc::new(RefCell::new(None::<Entity<OpenGpuiSceneHost>>));
    let scene_host = captured_host.clone();
    let scene_handle = stale_handle.clone();
    let scene_callbacks = callbacks.clone();
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                visible_viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        ExternalPortScene {
            host,
            stale_handle: scene_handle,
            callbacks: scene_callbacks,
        }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);
    let port = cx
        .debug_bounds("stale-mount-port")
        .expect("painted stale port bounds");

    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    assert_eq!(callbacks.get(), 0, "revoked painted callback must be inert");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
    });

    draw(cx);
    assert_eq!(attempts.get(), 2);
    let port = cx
        .debug_bounds("stale-mount-port")
        .expect("stale port remains rendered by the external owner");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    assert_eq!(callbacks.get(), 0, "old mount must not target replacement");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
    });
}

#[open_gpui::test]
fn revoked_port_prepaint_cannot_cancel_replacement_mount_drag(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(1))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let attempts = Rc::new(Cell::new(0_u32));
    let stale_handle = Rc::new(RefCell::new(None::<OpenGpuiPortHandle>));
    let stale_callbacks = Rc::new(Cell::new(0_u32));
    let live_phases = Rc::new(RefCell::new(Vec::new()));
    let factory_attempts = attempts.clone();
    let factory_handle = stale_handle.clone();
    let factory_phases = live_phases.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        if factory_handle.borrow().is_none() {
            *factory_handle.borrow_mut() = binding.read(cx).port_handle(port_id());
        }
        if factory_attempts.get() == 1 {
            return Err(OpenGpuiNodeMountError::new("retry interaction ABA fixture"));
        }
        Ok(cx.new(|_| InteractiveProbeNode {
            binding,
            phases: factory_phases.clone(),
        }))
    });

    let captured_host = Rc::new(RefCell::new(None::<Entity<OpenGpuiSceneHost>>));
    let scene_host = captured_host.clone();
    let scene_handle = stale_handle.clone();
    let scene_callbacks = stale_callbacks.clone();
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                visible_viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        ExternalPortScene {
            host,
            stale_handle: scene_handle,
            callbacks: scene_callbacks,
        }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);

    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));
    draw(cx);
    assert_eq!(attempts.get(), 2);

    let live_port = cx
        .debug_bounds("live-mount-port")
        .expect("replacement port bounds");
    cx.simulate_mouse_down(live_port.center(), MouseButton::Left, Default::default());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Dragging);
    });
    assert_eq!(
        live_phases.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start]
    );

    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);

    assert_eq!(stale_callbacks.get(), 0, "revoked callback remains inert");
    assert_eq!(
        live_phases.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start]
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Dragging);
    });

    cx.simulate_mouse_up(live_port.center(), MouseButton::Left, Default::default());
    assert_eq!(
        live_phases.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start, OpenGpuiPortDragPhase::Release,]
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
    });
}

fn binding_read(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
) -> OpenGpuiNodeBindingRead {
    cx.read_entity(binding, |binding, _| {
        binding.snapshot().read().expect("live binding read")
    })
}

fn assert_revoked_binding(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
) {
    cx.read_entity(binding, |binding, _| {
        assert!(matches!(
            binding.snapshot().read(),
            Err(OpenGpuiNodeBindingReadError::Retired { node }) if node == node_id()
        ));
        assert!(binding.port_handle(port_id()).is_none());
        assert!(binding.activity().is_revoked());
    });
    assert!(
        !cx.update_entity(binding, |binding, cx| binding.invalidate_layout(cx))
            .expect("retired layout invalidation is a benign no-op")
    );
}

fn assert_stale_reads_rejected(
    stale_execute: OpenGpuiNodeBindingRead,
    stale_handles: OpenGpuiNodeBindingRead,
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &mut open_gpui::VisualTestContext,
) {
    let stale_result = cx.update(|_, cx| {
        stale_execute.enqueue_command(cx, |_| panic!("retired read must not build a command"))
    });
    assert!(matches!(
        stale_result,
        Err(OpenGpuiNodeBindingReadError::Retired { node }) if node == node_id()
    ));
    assert!(matches!(
        stale_handles.port_connection_facts(port_id()),
        Err(OpenGpuiNodeBindingReadError::Retired { node }) if node == node_id()
    ));
    cx.read_entity(binding, |binding, _| {
        assert!(matches!(
            binding.port_handles(&stale_handles),
            Err(OpenGpuiNodeBindingReadError::Retired { node }) if node == node_id()
        ));
    });
}

fn assert_live_replacement(
    runtime: &GraphRuntime,
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
    value: serde_json::Value,
) {
    cx.read_entity(binding, |binding, _| {
        assert!(binding.snapshot().read().is_ok());
        assert!(binding.port_handle(port_id()).is_some());
        assert!(!binding.activity().is_revoked());
    });
    assert!(
        cx.update_entity(binding, |binding, cx| binding.invalidate_layout(cx))
            .expect("replacement layout invalidation")
    );

    let read = binding_read(cx, binding);
    let mut ticket = cx
        .update(|_, cx| {
            read.enqueue_command(cx, |read| {
                GraphCommand::set_node_data(read.graph_revision(), node_id(), value)
            })
        })
        .expect("replacement binding enqueues");
    draw(cx);
    let outcome = poll_command_ticket(&mut ticket).expect("replacement ticket resolves");
    outcome.applied().expect("replacement command applies");
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

fn poll_command_ticket(
    ticket: &mut jellyflow_open_gpui::OpenGpuiCommandTicket,
) -> jellyflow_open_gpui::OpenGpuiCommandTicketResult {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match std::future::Future::poll(std::pin::Pin::new(ticket), &mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("native command ticket is still pending"),
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn graph_with_nodes(count: usize) -> jellyflow::core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x6d_6f_75_6e_74));
    for index in 0..count {
        let id = NodeId::from_u128(index as u128 + 1);
        graph.insert_node(id, node(id == node_id()));
    }
    graph.insert_port(port_id(), port());
    graph.build().unwrap()
}

fn node(owns_port: bool) -> Node {
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
        ports: owns_port.then_some(port_id()).into_iter().collect(),
        data: serde_json::Value::Null,
    }
}

fn port() -> Port {
    Port {
        node: node_id(),
        key: PortKey::new("output"),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn node_id() -> NodeId {
    NodeId::from_u128(1)
}

fn port_id() -> PortId {
    PortId::from_u128(0x70_6f_72_74)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Mount capability node", RENDERER)
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
