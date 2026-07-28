use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
    OpenGpuiSceneHost,
};
use jellyflow_open_gpui::interaction::{
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragPhase, OpenGpuiPortEvent,
};
use jellyflow_open_gpui::navigation::{OpenGpuiInputBindings, OpenGpuiNavigationConfig};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
    commands::TopologyPort, graph::GraphRuntime, interaction::CanvasInteractionPhase,
    view::FrameMaterializationBudget, view::ViewCloseReason, view::ViewSessionError,
    view::ViewSessionOptions, view::WeakViewSession,
};
use open_gpui::{
    AnyWeakEntity, App, AppContext, Context, Entity, InteractiveElement, MouseButton,
    ParentElement, Render, Styled, Subscription, Task, WeakFocusHandle, Window, deferred, div, px,
};

const KIND: &str = "test.resource-node";
const RENDERER: &str = "test.native.resource-node";
const REPLACEMENT_KIND: &str = "test.replacement-resource-node";
const REPLACEMENT_RENDERER: &str = "test.native.replacement-resource-node";
const CYCLE_PORT_SELECTOR: &str = "jellyflow-cycle-resource-port";
const CAPACITY_PORT_SELECTOR: &str = "jellyflow-protected-capacity-port";

struct ResourceNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

struct ActivityResourceNode {
    _binding: Entity<OpenGpuiNodeBinding>,
    _activity: Vec<OpenGpuiActivityLease>,
}

struct CycleResourceNode {
    binding: Entity<OpenGpuiNodeBinding>,
    focus: open_gpui::FocusHandle,
    overlay: Entity<CycleOverlay>,
    pointer_starts: Rc<Cell<u64>>,
    owner_retirements: Rc<Cell<u64>>,
    host_closures: Rc<Cell<u64>>,
    _task: Task<()>,
    _subscription: Subscription,
    _activity: Vec<OpenGpuiActivityLease>,
}

struct CapacityPortNode {
    binding: Entity<OpenGpuiNodeBinding>,
    events: Rc<RefCell<Vec<OpenGpuiPortDragPhase>>>,
}

struct CycleOverlay;

struct CycleSubscriptionSource;

struct TaskDropProbe(Rc<Cell<u64>>);

#[derive(Clone)]
struct CycleResourceProbes {
    weak_entities: Rc<RefCell<Vec<AnyWeakEntity>>>,
    weak_overlays: Rc<RefCell<Vec<AnyWeakEntity>>>,
    weak_focus_handles: Rc<RefCell<Vec<WeakFocusHandle>>>,
    revocations: Rc<Cell<u64>>,
    revocation_panic: Option<Rc<Cell<u64>>>,
    task_cancellations: Rc<Cell<u64>>,
    subscription_callbacks: Rc<Cell<u64>>,
    pointer_starts: Rc<Cell<u64>>,
    owner_retirements: Rc<Cell<u64>>,
    host_closures: Rc<Cell<u64>>,
    subscription_source: Entity<CycleSubscriptionSource>,
}

impl Drop for TaskDropProbe {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_add(1));
    }
}

struct DetachableHostScene {
    host: Entity<OpenGpuiSceneHost>,
    attached: Rc<Cell<bool>>,
}

struct ReplaceableHostScene {
    host: Option<Entity<OpenGpuiSceneHost>>,
}

impl Render for ResourceNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("resource node")
    }
}

impl Render for ActivityResourceNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("activity resource node")
    }
}

impl Render for CycleResourceNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let focus = self.focus.clone();
        let pointer_starts = self.pointer_starts.clone();
        let owner_retirements = self.owner_retirements.clone();
        let host_closures = self.host_closures.clone();
        let binding = self.binding.read(cx);
        let port = binding.authoring().ok().and_then(|authoring| {
            authoring
                .port(cycle_port_id())
                .map(|port| port.handle().clone())
        });
        let overlay_boundary = binding.overlay_boundary();

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(8.0))
                    .track_focus(&focus)
                    .child("cycle resource node"),
            )
            .children(port.map(|handle| {
                div().absolute().right(px(8.0)).top(px(43.0)).child(
                    handle.interactive_with_observer(
                        div()
                            .debug_selector(|| CYCLE_PORT_SELECTOR.to_owned())
                            .w(px(14.0))
                            .h(px(14.0)),
                        move |event, _, _| {
                            let OpenGpuiPortEvent::Pointer(event) = event else {
                                return;
                            };
                            match event.phase() {
                                OpenGpuiPortDragPhase::Start => {
                                    pointer_starts.set(pointer_starts.get().saturating_add(1));
                                }
                                OpenGpuiPortDragPhase::Cancel(
                                    OpenGpuiPortDragCancelReason::OwnerRetired,
                                ) => {
                                    owner_retirements
                                        .set(owner_retirements.get().saturating_add(1));
                                }
                                OpenGpuiPortDragPhase::Cancel(
                                    OpenGpuiPortDragCancelReason::HostClosed,
                                ) => {
                                    host_closures.set(host_closures.get().saturating_add(1));
                                }
                                _ => {}
                            }
                        },
                    ),
                )
            }))
            .child(deferred(overlay_boundary.wrap(self.overlay.clone())))
    }
}

impl Render for CapacityPortNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let events = self.events.clone();
        let handle = self
            .binding
            .read(cx)
            .authoring()
            .ok()
            .and_then(|authoring| {
                authoring
                    .port(capacity_port_id())
                    .map(|port| port.handle().clone())
            });
        div().relative().size_full().children(handle.map(|handle| {
            div().absolute().right(px(8.0)).top(px(43.0)).child(
                handle.interactive_with_observer(
                    div()
                        .debug_selector(|| CAPACITY_PORT_SELECTOR.to_owned())
                        .w(px(14.0))
                        .h(px(14.0)),
                    move |event, _, _| {
                        if let OpenGpuiPortEvent::Pointer(event) = event {
                            events.borrow_mut().push(event.phase());
                        }
                    },
                ),
            )
        }))
    }
}

impl Render for CycleOverlay {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div()
            .absolute()
            .left(px(36.0))
            .top(px(72.0))
            .w(px(24.0))
            .h(px(12.0))
            .child("overlay")
    }
}

impl Render for DetachableHostScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().children(self.attached.get().then(|| self.host.clone()))
    }
}

impl Render for ReplaceableHostScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().children(self.host.clone())
    }
}

#[open_gpui::test]
fn protected_nodes_ignore_zero_required_budget_then_resume_ordinary_culling(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let focus_lease = Rc::new(RefCell::new(None::<OpenGpuiActivityLease>));
    let capture_lease = Rc::new(RefCell::new(None::<OpenGpuiActivityLease>));
    let task_lease = Rc::new(RefCell::new(None::<OpenGpuiActivityLease>));
    let task_revocations = Rc::new(Cell::new(0_u32));
    let factory_focus = focus_lease.clone();
    let factory_capture = capture_lease.clone();
    let factory_task = task_lease.clone();
    let factory_revocations = task_revocations.clone();
    let captured_host = Rc::new(RefCell::new(None::<Entity<OpenGpuiSceneHost>>));
    let scene_host = captured_host.clone();
    let attached = Rc::new(Cell::new(true));
    let scene_attached = attached.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let activity = binding.read(cx).activity().clone();
        *factory_focus.borrow_mut() = Some(activity.acquire(OpenGpuiActivityKind::Focus).unwrap());
        *factory_capture.borrow_mut() = Some(
            activity
                .acquire(OpenGpuiActivityKind::GraphInteraction)
                .unwrap(),
        );
        let revocations = factory_revocations.clone();
        *factory_task.borrow_mut() = Some(
            activity
                .acquire_with_revocation(OpenGpuiActivityKind::Task, move || {
                    revocations.set(revocations.get().saturating_add(1));
                })
                .unwrap(),
        );
        cx.new(|_| ResourceNode { _binding: binding })
    });

    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                visible_viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        DetachableHostScene {
            host,
            attached: scene_attached,
        }
    });
    let host = captured_host.borrow().clone().expect("scene host");
    draw(cx);

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        let configured = FrameMaterializationBudget::default();
        assert!(
            host.set_frame_materialization_budget(
                FrameMaterializationBudget::new(
                    configured.visible_nodes(),
                    0,
                    configured.routing_dependency_nodes(),
                )
                .with_visible_node_scan_limit(configured.visible_node_scan_limit()),
                cx,
            )
        );
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.frame_materialization_budget().required_nodes(), 0);
        assert_eq!(
            host.last_frame_materialization_overflow()
                .expect("a rendered frame must publish overflow diagnostics")
                .required_nodes(),
            0
        );
    });

    drop(focus_lease.borrow_mut().take());
    drop(capture_lease.borrow_mut().take());
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
    });

    attached.set(false);
    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("resource removal snapshot")
                .version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();

    assert_eq!(task_revocations.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
    });

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("resource restoration snapshot")
                .version(),
            TopologyEdit::new().add_node(node_id(), node(), []),
        ))
        .into_result()
        .unwrap();
    attached.set(true);
    cx.update_entity(&scene, |_, cx| cx.notify());
    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(visible_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
    });
}

#[open_gpui::test]
fn coalesced_reincarnation_hard_retires_active_node_without_waiting_for_draw(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let revocations = Rc::new(Cell::new(0_u32));
    let weak_entity = Rc::new(RefCell::new(None::<AnyWeakEntity>));
    let factory_revocations = revocations.clone();
    let factory_weak_entity = weak_entity.clone();
    let captured_host = Rc::new(RefCell::new(None::<Entity<OpenGpuiSceneHost>>));
    let scene_host = captured_host.clone();
    let attached = Rc::new(Cell::new(true));
    let scene_attached = attached.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let activity = binding.read(cx).activity().clone();
        let revocations = factory_revocations.clone();
        let lease = activity
            .acquire_with_revocation(OpenGpuiActivityKind::GraphInteraction, move || {
                revocations.set(revocations.get().saturating_add(1));
            })
            .unwrap();
        let entity = cx.new(|_| ActivityResourceNode {
            _binding: binding,
            _activity: vec![lease],
        });
        *factory_weak_entity.borrow_mut() = Some(entity.downgrade().into());
        entity
    });

    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                visible_viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        DetachableHostScene {
            host,
            attached: scene_attached,
        }
    });
    let host = captured_host.borrow().clone().expect("scene host");
    draw(cx);

    attached.set(false);
    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("detached resource removal snapshot")
                .version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    let removed_version = runtime
        .snapshot()
        .expect("detached resource undo snapshot")
        .version();
    runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .unwrap();
    cx.run_until_parked();

    assert_eq!(revocations.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
    cx.update(|_, _| {});
    cx.run_until_parked();
    weak_entity
        .borrow()
        .as_ref()
        .expect("mounted node weak entity")
        .assert_released();
}

#[open_gpui::test]
fn protected_instance_budget_is_distinct_bounded_and_reusable(cx: &mut open_gpui::TestAppContext) {
    let node_count = OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1;
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(node_count))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let registries = Rc::new(RefCell::new(
        BTreeMap::<NodeId, OpenGpuiNodeActivityHandle>::new(),
    ));
    let leases = Rc::new(RefCell::new(
        BTreeMap::<NodeId, OpenGpuiActivityLease>::new(),
    ));
    let failures = Rc::new(RefCell::new(BTreeMap::new()));
    let factory_registries = registries.clone();
    let factory_leases = leases.clone();
    let factory_failures = failures.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let binding_entity = binding;
        let binding = binding_entity.read(cx);
        let node = binding.authoring().expect("live authoring view").node_id();
        let activity = binding.activity().clone();
        factory_registries
            .borrow_mut()
            .insert(node, activity.clone());
        match activity.acquire(OpenGpuiActivityKind::Focus) {
            Ok(lease) => {
                factory_leases.borrow_mut().insert(node, lease);
            }
            Err(error) => {
                factory_failures.borrow_mut().insert(node, error);
            }
        }
        cx.new(|_| ResourceNode {
            _binding: binding_entity,
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

    assert_eq!(leases.borrow().len(), OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
    assert_eq!(failures.borrow().len(), 1);
    let overflow_node = *failures.borrow().keys().next().unwrap();
    let multiply_protected_node = *leases.borrow().keys().next().unwrap();
    let _ime_lease = registries.borrow()[&multiply_protected_node]
        .acquire(OpenGpuiActivityKind::Ime)
        .unwrap();
    assert_eq!(
        failures.borrow()[&overflow_node],
        OpenGpuiActivityAcquireError::ProtectedCapacityExceeded {
            capacity: OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        },
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        );
        assert_eq!(
            host.resource_snapshot().protected_leases(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1,
        );
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().active_nodes(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        );
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
    });

    let released_node = *leases.borrow().keys().nth(1).unwrap();
    drop(leases.borrow_mut().remove(&released_node));
    let replacement = registries.borrow()[&overflow_node]
        .acquire(OpenGpuiActivityKind::Focus)
        .unwrap();
    leases.borrow_mut().insert(overflow_node, replacement);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().active_nodes(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        );
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        );
    });
}

#[open_gpui::test]
fn sixty_fifth_real_port_drag_cancels_without_side_effects_then_retries_after_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(protected_capacity_port_graph())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let leases = Rc::new(RefCell::new(
        BTreeMap::<NodeId, OpenGpuiActivityLease>::new(),
    ));
    let events = Rc::new(RefCell::new(Vec::new()));
    let factory_leases = leases.clone();
    let factory_events = events.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = binding
            .read(cx)
            .authoring()
            .expect("live authoring view")
            .node_id();
        if node != capacity_port_node_id() {
            let activity = binding.read(cx).activity().clone();
            let lease = activity.acquire(OpenGpuiActivityKind::Focus).unwrap();
            factory_leases.borrow_mut().insert(node, lease);
        }
        cx.new(|_| CapacityPortNode {
            binding,
            events: factory_events.clone(),
        })
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
        .with_navigation_config(
            OpenGpuiNavigationConfig::default()
                .with_input_bindings(OpenGpuiInputBindings::default().with_connect_on_click(false)),
        )
    });
    draw(cx);
    assert_eq!(leases.borrow().len(), OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);

    let previous_focus = cx.update(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        focus
    });
    let graph_before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let port = cx
        .debug_bounds(CAPACITY_PORT_SELECTOR)
        .expect("capacity port bounds");

    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert_eq!(
        events.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Cancel(
            OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded,
        )],
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus.clone())));
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);

    let released_node = *leases.borrow().keys().next().expect("one protected owner");
    drop(leases.borrow_mut().remove(&released_node));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY - 1,
        );
    });

    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_some()));
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert_eq!(
        events.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded,),
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Release,
        ],
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY - 1,
        );
    });
}

#[open_gpui::test]
fn eligible_lru_evicts_oldest_instances_and_keeps_strong_references_bounded(
    cx: &mut open_gpui::TestAppContext,
) {
    let node_count = OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY + 2;
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(node_count))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let mounts = Rc::new(RefCell::new(BTreeMap::<NodeId, u32>::new()));
    let factory_mounts = mounts.clone();

    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = binding
            .read(cx)
            .authoring()
            .expect("live authoring view")
            .node_id();
        let count = factory_mounts.borrow().get(&node).copied().unwrap_or(0);
        factory_mounts
            .borrow_mut()
            .insert(node, count.saturating_add(1));
        cx.new(|_| ResourceNode { _binding: binding })
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
        assert_eq!(host.resource_snapshot().active_nodes(), node_count);
        assert_eq!(host.resource_snapshot().strong_instances(), node_count);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(
            host.resource_snapshot().parked_nodes(),
            OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
        );
        assert_eq!(
            host.resource_snapshot().eligible_parked_nodes(),
            OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
        );
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
        );
        assert!(
            host.resource_snapshot().eligible_lru_entries()
                <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
        );
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(visible_viewport());
        cx.notify();
    });
    draw(cx);

    assert_eq!(mounts.borrow()[&NodeId::from_u128(1)], 2);
    assert_eq!(mounts.borrow()[&NodeId::from_u128(2)], 2);
    assert_eq!(mounts.borrow()[&NodeId::from_u128(3)], 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), node_count);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
    });
}

#[open_gpui::test]
fn thousand_lifecycle_cycles_return_native_resources_to_baseline(
    cx: &mut open_gpui::TestAppContext,
) {
    const CYCLES: u64 = 1_000;
    const EXPLICIT_LEASES_PER_MOUNT: u64 = 6;
    const MOUNTS_PER_CYCLE: u64 = 3;

    let weak_entities = Rc::new(RefCell::new(Vec::<AnyWeakEntity>::new()));
    let weak_overlays = Rc::new(RefCell::new(Vec::<AnyWeakEntity>::new()));
    let weak_focus_handles = Rc::new(RefCell::new(Vec::<WeakFocusHandle>::new()));
    let revocations = Rc::new(Cell::new(0_u64));
    let task_cancellations = Rc::new(Cell::new(0_u64));
    let subscription_callbacks = Rc::new(Cell::new(0_u64));
    let pointer_starts = Rc::new(Cell::new(0_u64));
    let owner_retirements = Rc::new(Cell::new(0_u64));
    let host_closures = Rc::new(Cell::new(0_u64));
    let subscription_source = cx.new(|_| CycleSubscriptionSource);
    let probes = CycleResourceProbes {
        weak_entities: weak_entities.clone(),
        weak_overlays: weak_overlays.clone(),
        weak_focus_handles: weak_focus_handles.clone(),
        revocations: revocations.clone(),
        revocation_panic: None,
        task_cancellations: task_cancellations.clone(),
        subscription_callbacks: subscription_callbacks.clone(),
        pointer_starts: pointer_starts.clone(),
        owner_retirements: owner_retirements.clone(),
        host_closures: host_closures.clone(),
        subscription_source: subscription_source.clone(),
    };
    let primary_mounts = Rc::new(Cell::new(0_u64));
    let replacement_mounts = Rc::new(Cell::new(0_u64));
    let (scene, cx) = cx.add_window_view(|_, _| ReplaceableHostScene { host: None });

    for cycle in 0..CYCLES {
        let graph_id = GraphId::from_u128(u128::from(cycle) + 3);
        let runtime = GraphRuntime::builder(cycle_catalog())
            .build(cycle_graph(graph_id, KIND))
            .unwrap();
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        let weak_view = view.downgrade();
        let geometry_baseline = published_geometry_counts(&weak_view);
        assert_eq!(geometry_baseline, (0, 0, 0));
        let registry = cycle_registry(
            probes.clone(),
            primary_mounts.clone(),
            replacement_mounts.clone(),
        );
        assert_eq!(registry.len(), 2);
        let host = cx
            .update(|_, cx| cx.new(|_| OpenGpuiSceneHost::new(view, registry, visible_viewport())));
        let weak_host: AnyWeakEntity = host.downgrade().into();
        cx.update_entity(&scene, |scene, cx| {
            scene.host = Some(host.clone());
            cx.notify();
        });
        draw(cx);

        assert_eq!(primary_mounts.get(), cycle.saturating_mul(2) + 1);
        assert_eq!(replacement_mounts.get(), cycle);
        assert_eq!(published_geometry_counts(&weak_view), (1, 1, 0));
        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), 1);
            assert_eq!(host.resource_snapshot().strong_instances(), 1);
            assert_eq!(host.resource_snapshot().protected_instances(), 1);
        });

        start_cycle_capture(cx);
        assert_eq!(pointer_starts.get(), cycle.saturating_mul(3) + 1);
        runtime
            .execute(GraphCommand::topology(
                runtime
                    .snapshot()
                    .expect("cycle removal snapshot")
                    .version(),
                TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
            ))
            .into_result()
            .unwrap();
        draw(cx);
        assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
        assert_eq!(owner_retirements.get(), 0);
        assert_eq!(task_cancellations.get(), cycle.saturating_mul(3) + 1);
        assert_eq!(
            revocations.get(),
            (cycle.saturating_mul(3) + 1).saturating_mul(EXPLICIT_LEASES_PER_MOUNT),
        );
        assert_eq!(published_geometry_counts(&weak_view), geometry_baseline);
        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), 0);
            assert_eq!(host.resource_snapshot().strong_instances(), 0);
            assert_eq!(host.resource_snapshot().protected_instances(), 0);
            assert_eq!(host.resource_snapshot().protected_leases(), 0);
        });

        runtime
            .execute(GraphCommand::topology(
                runtime
                    .snapshot()
                    .expect("cycle restoration snapshot")
                    .version(),
                TopologyEdit::new().add_node(
                    node_id(),
                    cycle_node(KIND),
                    [TopologyPort::new(cycle_port_id(), cycle_port())],
                ),
            ))
            .into_result()
            .unwrap();
        draw(cx);
        assert_eq!(primary_mounts.get(), cycle.saturating_mul(2) + 2);
        assert_eq!(published_geometry_counts(&weak_view), (1, 1, 0));

        start_cycle_capture(cx);
        assert_eq!(pointer_starts.get(), cycle.saturating_mul(3) + 2);
        let replacement_graph = cycle_graph(graph_id, REPLACEMENT_KIND);
        let replacement = runtime
            .catalog()
            .canonicalize_graph(&replacement_graph)
            .unwrap();
        runtime.replace_document(replacement).unwrap();
        draw(cx);
        assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
        assert_eq!(runtime.snapshot().unwrap().document_epoch(), 1);
        assert_eq!(owner_retirements.get(), 0);
        assert_eq!(task_cancellations.get(), cycle.saturating_mul(3) + 2);
        assert_eq!(
            revocations.get(),
            (cycle.saturating_mul(3) + 2).saturating_mul(EXPLICIT_LEASES_PER_MOUNT),
        );
        assert_eq!(replacement_mounts.get(), cycle + 1);
        assert_eq!(published_geometry_counts(&weak_view), (1, 1, 0));
        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), 1);
            assert_eq!(host.resource_snapshot().strong_instances(), 1);
            assert_eq!(host.resource_snapshot().protected_instances(), 1);
        });
        flush_effect_cycle(cx);
        assert_released_entities(&weak_entities, 2);
        assert_released_entities(&weak_overlays, 2);
        assert_released_focus_handles(&weak_focus_handles, 2);

        cx.update_entity(&subscription_source, |_, cx| cx.notify());
        cx.run_until_parked();
        assert_eq!(subscription_callbacks.get(), cycle + 1);

        start_cycle_capture(cx);
        assert_eq!(pointer_starts.get(), cycle.saturating_mul(3) + 3);
        cx.update(|window, cx| {
            host.update(cx, |host, cx| host.close_in_window(window, cx))
                .expect("host close");
            assert!(window.captured_hitbox().is_none());
        });
        draw(cx);

        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().active_nodes(), 0);
            assert_eq!(host.resource_snapshot().parked_nodes(), 0);
            assert_eq!(host.resource_snapshot().strong_instances(), 0);
            assert_eq!(host.resource_snapshot().protected_instances(), 0);
            assert_eq!(host.resource_snapshot().protected_leases(), 0);
        });
        assert_eq!(
            revocations.get(),
            (cycle + 1)
                .saturating_mul(MOUNTS_PER_CYCLE)
                .saturating_mul(EXPLICIT_LEASES_PER_MOUNT),
        );
        assert_eq!(
            task_cancellations.get(),
            (cycle + 1).saturating_mul(MOUNTS_PER_CYCLE),
        );
        assert_eq!(owner_retirements.get(), 0);
        assert_eq!(host_closures.get(), 0);
        assert!(matches!(
            weak_view.geometry_snapshot(),
            Err(ViewSessionError::Closed {
                reason: ViewCloseReason::HostClosed,
            }),
        ));
        assert_eq!(runtime.live_view_count(), 0);
        flush_effect_cycle(cx);
        assert_released_entities(&weak_entities, 1);
        assert_released_entities(&weak_overlays, 1);
        assert_released_focus_handles(&weak_focus_handles, 1);

        cx.update_entity(&subscription_source, |_, cx| cx.notify());
        cx.run_until_parked();
        assert_eq!(subscription_callbacks.get(), cycle + 1);

        cx.update_entity(&scene, |scene, cx| {
            scene.host = None;
            cx.notify();
        });
        draw(cx);
        drop(host);
        flush_effect_cycle(cx);
        weak_host.assert_released();
        assert!(weak_entities.borrow().is_empty());
        assert!(weak_overlays.borrow().is_empty());
        assert!(weak_focus_handles.borrow().is_empty());
    }

    assert_eq!(primary_mounts.get(), CYCLES.saturating_mul(2));
    assert_eq!(replacement_mounts.get(), CYCLES);
    assert_eq!(
        revocations.get(),
        CYCLES
            .saturating_mul(MOUNTS_PER_CYCLE)
            .saturating_mul(EXPLICIT_LEASES_PER_MOUNT),
    );
    assert_eq!(
        task_cancellations.get(),
        CYCLES.saturating_mul(MOUNTS_PER_CYCLE),
    );
    assert_eq!(subscription_callbacks.get(), CYCLES);
    assert_eq!(pointer_starts.get(), CYCLES.saturating_mul(3));
    assert_eq!(owner_retirements.get(), 0);
    assert_eq!(host_closures.get(), 0);
}

#[open_gpui::test]
fn dropping_last_host_entity_releases_live_native_resources_after_revocation_panic(
    cx: &mut open_gpui::TestAppContext,
) {
    const EXPLICIT_LEASES: u64 = 6;

    let weak_entities = Rc::new(RefCell::new(Vec::<AnyWeakEntity>::new()));
    let weak_overlays = Rc::new(RefCell::new(Vec::<AnyWeakEntity>::new()));
    let weak_focus_handles = Rc::new(RefCell::new(Vec::<WeakFocusHandle>::new()));
    let revocations = Rc::new(Cell::new(0_u64));
    let revocation_panics = Rc::new(Cell::new(0_u64));
    let task_cancellations = Rc::new(Cell::new(0_u64));
    let subscription_callbacks = Rc::new(Cell::new(0_u64));
    let subscription_source = cx.new(|_| CycleSubscriptionSource);
    let probes = CycleResourceProbes {
        weak_entities: weak_entities.clone(),
        weak_overlays: weak_overlays.clone(),
        weak_focus_handles: weak_focus_handles.clone(),
        revocations: revocations.clone(),
        revocation_panic: Some(revocation_panics.clone()),
        task_cancellations: task_cancellations.clone(),
        subscription_callbacks: subscription_callbacks.clone(),
        pointer_starts: Rc::new(Cell::new(0)),
        owner_retirements: Rc::new(Cell::new(0)),
        host_closures: Rc::new(Cell::new(0)),
        subscription_source: subscription_source.clone(),
    };
    let runtime = GraphRuntime::builder(cycle_catalog())
        .build(cycle_graph(GraphId::from_u128(2), KIND))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let registry = cycle_registry(probes, Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (scene, cx) = cx.add_window_view(|_, _| ReplaceableHostScene { host: None });
    let host =
        cx.update(|_, cx| cx.new(|_| OpenGpuiSceneHost::new(view, registry, visible_viewport())));
    let weak_host: AnyWeakEntity = host.downgrade().into();
    cx.update_entity(&scene, |scene, cx| {
        scene.host = Some(host.clone());
        cx.notify();
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().strong_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });
    assert_eq!(runtime.live_view_count(), 1);
    assert_eq!(weak_entities.borrow().len(), 1);
    assert_eq!(weak_overlays.borrow().len(), 1);
    assert_eq!(weak_focus_handles.borrow().len(), 1);
    assert_eq!(revocations.get(), 0);
    assert_eq!(task_cancellations.get(), 0);

    cx.update_entity(&subscription_source, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(subscription_callbacks.get(), 1);

    cx.update_entity(&scene, |scene, cx| {
        scene.host = None;
        cx.notify();
    });
    draw(cx);
    drop(host);
    flush_effect_cycle(cx);

    weak_host.assert_released();
    assert_eq!(revocations.get(), EXPLICIT_LEASES);
    assert_eq!(revocation_panics.get(), 1);
    assert_eq!(task_cancellations.get(), 1);
    assert_eq!(runtime.live_view_count(), 0);
    assert_released_entities(&weak_entities, 1);
    assert_released_entities(&weak_overlays, 1);
    assert_released_focus_handles(&weak_focus_handles, 1);

    cx.update_entity(&subscription_source, |_, cx| cx.notify());
    flush_effect_cycle(cx);
    assert_eq!(subscription_callbacks.get(), 1);
    assert_eq!(revocations.get(), EXPLICIT_LEASES);
    assert_eq!(revocation_panics.get(), 1);
    assert_eq!(task_cancellations.get(), 1);
    assert_eq!(runtime.live_view_count(), 0);
    assert!(weak_entities.borrow().is_empty());
    assert!(weak_overlays.borrow().is_empty());
    assert!(weak_focus_handles.borrow().is_empty());
}

fn cycle_registry(
    probes: CycleResourceProbes,
    primary_mounts: Rc<Cell<u64>>,
    replacement_mounts: Rc<Cell<u64>>,
) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(
        RENDERER,
        cycle_resource_factory(probes.clone(), primary_mounts),
    );
    registry.register(
        REPLACEMENT_RENDERER,
        cycle_resource_factory(probes, replacement_mounts),
    );
    registry.seal().expect("valid test registry")
}

fn cycle_resource_factory(
    probes: CycleResourceProbes,
    mounts: Rc<Cell<u64>>,
) -> impl Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<CycleResourceNode> {
    move |binding, _window, cx| {
        mounts.set(mounts.get().saturating_add(1));
        let activity = binding.read(cx).activity().clone();
        let leases = [
            OpenGpuiActivityKind::Focus,
            OpenGpuiActivityKind::Ime,
            OpenGpuiActivityKind::Overlay,
            OpenGpuiActivityKind::Draft,
            OpenGpuiActivityKind::Task,
            OpenGpuiActivityKind::Subscription,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let revocations = probes.revocations.clone();
            let revocation_panic = if index == 0 {
                probes.revocation_panic.clone()
            } else {
                None
            };
            activity
                .acquire_with_revocation(kind, move || {
                    revocations.set(revocations.get().saturating_add(1));
                    if let Some(revocation_panics) = revocation_panic {
                        revocation_panics.set(revocation_panics.get().saturating_add(1));
                        panic!("injected activity revocation panic");
                    }
                })
                .unwrap()
        })
        .collect();
        let focus = cx.focus_handle().tab_stop(true);
        probes
            .weak_focus_handles
            .borrow_mut()
            .push(focus.downgrade());
        let overlay = cx.new(|_| CycleOverlay);
        probes
            .weak_overlays
            .borrow_mut()
            .push(overlay.downgrade().into());
        let task_cancellations = probes.task_cancellations.clone();
        let task = cx.foreground_executor().spawn(async move {
            let _probe = TaskDropProbe(task_cancellations);
            future::pending::<()>().await;
        });
        let subscription_callbacks = probes.subscription_callbacks.clone();
        let subscription = cx.observe(&probes.subscription_source, move |_, _| {
            subscription_callbacks.set(subscription_callbacks.get().saturating_add(1));
        });
        let entity = cx.new(|_| CycleResourceNode {
            binding,
            focus,
            overlay,
            pointer_starts: probes.pointer_starts.clone(),
            owner_retirements: probes.owner_retirements.clone(),
            host_closures: probes.host_closures.clone(),
            _task: task,
            _subscription: subscription,
            _activity: leases,
        });
        probes
            .weak_entities
            .borrow_mut()
            .push(entity.downgrade().into());
        entity
    }
}

fn start_cycle_capture(cx: &mut open_gpui::VisualTestContext) {
    let port = cx
        .debug_bounds(CYCLE_PORT_SELECTOR)
        .expect("cycle port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
}

fn assert_released_entities(resources: &Rc<RefCell<Vec<AnyWeakEntity>>>, count: usize) {
    let released = resources.borrow_mut().drain(..count).collect::<Vec<_>>();
    assert_eq!(released.len(), count);
    for resource in released {
        resource.assert_released();
    }
}

fn assert_released_focus_handles(resources: &Rc<RefCell<Vec<WeakFocusHandle>>>, count: usize) {
    let released = resources.borrow_mut().drain(..count).collect::<Vec<_>>();
    assert_eq!(released.len(), count);
    for resource in released {
        assert!(resource.upgrade().is_none());
    }
}

fn published_geometry_counts(view: &WeakViewSession) -> (usize, usize, usize) {
    let geometry = view.geometry_snapshot().expect("open view geometry");
    (
        geometry.nodes().len(),
        geometry.ports().len(),
        geometry.routes().len(),
    )
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn flush_effect_cycle(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|_, _| {});
    cx.run_until_parked();
}

fn graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node_id(), node());
    graph.build().unwrap()
}

fn cycle_graph(graph_id: GraphId, kind: &str) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(graph_id);
    graph.insert_node(node_id(), cycle_node(kind));
    graph.insert_port(cycle_port_id(), cycle_port());
    graph.build().unwrap()
}

fn graph_with_nodes(count: usize) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    for index in 0..count {
        graph.insert_node(NodeId::from_u128(index as u128 + 1), node());
    }
    graph.build().unwrap()
}

fn protected_capacity_port_graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        let mut filler = node();
        filler.pos = CanvasPoint { x: 400.0, y: 300.0 };
        graph.insert_node(NodeId::from_u128(index as u128 + 1), filler);
    }
    let mut target = node();
    target.ports.push(capacity_port_id());
    graph.insert_node(capacity_port_node_id(), target);
    graph.insert_port(
        capacity_port_id(),
        Port {
            node: capacity_port_node_id(),
            key: PortKey::new("capacity-output"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.build().unwrap()
}

fn node_id() -> NodeId {
    NodeId::from_u128(10)
}

fn cycle_port_id() -> PortId {
    PortId::from_u128(20)
}

fn capacity_port_node_id() -> NodeId {
    NodeId::from_u128(10_000)
}

fn capacity_port_id() -> PortId {
    PortId::from_u128(20_000)
}

fn cycle_node(kind: &str) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        ports: vec![cycle_port_id()],
        data: serde_json::json!({ "label": "cycle resource" }),
        ..node()
    }
}

fn cycle_port() -> Port {
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

fn node() -> Node {
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
        data: serde_json::Value::Null,
    }
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Resource node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
        .unwrap();
    catalog
}

fn cycle_catalog() -> NodeCatalog {
    let mut catalog = catalog();
    catalog
        .register(
            NodeKindSpec::builder(
                REPLACEMENT_KIND,
                "Replacement resource node",
                REPLACEMENT_RENDERER,
            )
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
