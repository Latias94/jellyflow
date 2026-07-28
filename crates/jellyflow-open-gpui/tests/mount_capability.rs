use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::activity::OpenGpuiNodeActivityHandle;
use jellyflow_open_gpui::authoring::{
    MutationDispatchError, MutationLayoutEffect, MutationLayoutSettlement, MutationRequest,
    MutationSettlement, OpenGpuiManagedMutation, OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError,
    OpenGpuiNodeBinding, OpenGpuiNodeDragHandle, OpenGpuiPortHandle,
};
use jellyflow_open_gpui::host::{OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OpenGpuiSceneHost};
use jellyflow_open_gpui::interaction::{OpenGpuiPortDragPhase, OpenGpuiPortEvent};
use jellyflow_open_gpui::navigation::{OpenGpuiInputBindings, OpenGpuiNavigationConfig};
use jellyflow_open_gpui::registry::{OpenGpuiNodeMountError, OpenGpuiNodeRegistry};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, graph::GraphRuntime, interaction::CanvasInteractionPhase,
    view::ViewCloseReason, view::ViewSessionOptions,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Render, Styled, VisualContext, Window, div, px,
};

const KIND: &str = "test.mount-capability-node";
const RENDERER: &str = "test.native.mount-capability-node";

struct ProbeNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

struct MutationHarness {
    mutation: OpenGpuiManagedMutation<serde_json::Value>,
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

struct InteractiveDragProbeNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

struct ExternalDragScene {
    host: Entity<OpenGpuiSceneHost>,
    stale_handle: Rc<RefCell<Option<OpenGpuiNodeDragHandle>>>,
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
        let handle = self
            .binding
            .read(cx)
            .authoring()
            .ok()
            .and_then(|authoring| authoring.port(port_id()).map(|port| port.handle().clone()));
        let phases = self.phases.clone();
        div().relative().size_full().children(handle.map(|handle| {
            div()
                .absolute()
                .right(px(8.0))
                .top(px(40.0))
                .child(
                    handle.interactive_with_observer(
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
                .interactive_with_observer(
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

impl Render for InteractiveDragProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.binding.read(cx).drag_handle().wrap(
            div()
                .debug_selector(|| "live-mount-drag".to_owned())
                .size_full(),
        )
    }
}

impl Render for ExternalDragScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let stale_drag = self.stale_handle.borrow().clone().map(|handle| {
            handle
                .wrap(
                    div()
                        .debug_selector(|| "stale-mount-drag".to_owned())
                        .w(px(32.0))
                        .h(px(32.0)),
                )
                .into_any_element()
        });
        div().relative().size_full().child(self.host.clone()).child(
            div()
                .absolute()
                .right(px(20.0))
                .top(px(80.0))
                .children(stale_drag),
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
        let node = binding
            .read(cx)
            .authoring()
            .expect("new mount opens authoring")
            .node_id();
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
    let old_instance = cx.read_entity(&old, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    let stale_execute = binding_authoring(cx, &old);
    let stale_facts = binding_authoring(cx, &old);
    let stale_mutation = mutation_harness(cx, &old);
    cx.read_entity(&old, |binding, _| {
        assert!(binding.authoring().unwrap().port(port_id()).is_some());
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
    let new_instance = cx.read_entity(&new, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    assert_eq!(
        old_instance, new_instance,
        "runtime identity survives eviction"
    );
    assert_revoked_binding(cx, &old);
    assert_stale_authoring_rejected(stale_execute, stale_facts, &old, &stale_mutation, cx);
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
    let old_instance = cx.read_entity(&old, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    let stale_execute = binding_authoring(cx, &old);
    let stale_facts = binding_authoring(cx, &old);
    let stale_mutation = mutation_harness(cx, &old);
    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));
    assert_revoked_binding(cx, &old);

    draw(cx);
    assert_eq!(attempts.get(), 2);
    let new = bindings.borrow()[1].clone();
    let new_instance = cx.read_entity(&new, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    assert_eq!(old_instance, new_instance, "retry reuses runtime identity");
    assert_revoked_binding(cx, &old);
    assert_stale_authoring_rejected(stale_execute, stale_facts, &old, &stale_mutation, cx);
    assert_live_replacement(&runtime, cx, &new, serde_json::json!("after retry"));
}

#[open_gpui::test]
fn authoring_rejects_mismatched_and_stale_commands_before_journal_admission(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph_with_nodes(1))
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let captured_binding = Rc::new(RefCell::new(None::<Entity<OpenGpuiNodeBinding>>));
    let factory_binding = captured_binding.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        *factory_binding.borrow_mut() = Some(binding.clone());
        cx.new(|_| ProbeNode { _binding: binding })
    });

    let (_host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    let binding = captured_binding
        .borrow()
        .clone()
        .expect("captured live binding");

    let foreign_runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(1))
        .expect("foreign runtime");
    let foreign_version = foreign_runtime
        .snapshot()
        .expect("foreign runtime snapshot")
        .version();
    let mismatched = binding_authoring(cx, &binding);
    let mismatch_harness = mutation_harness(cx, &binding);
    let mismatch_value = serde_json::json!("mismatch");
    let mismatch = cx.update_entity(&mismatch_harness, |harness, cx| {
        harness.mutation.dispatch(
            mismatched,
            GraphCommand::set_node_data(foreign_version, node_id(), mismatch_value.clone()),
            MutationRequest::new(mismatch_value),
            MutationLayoutEffect::Preserve,
            cx,
        )
    });
    assert!(matches!(
        mismatch,
        Err(MutationDispatchError::NotDispatched { .. })
    ));
    draw(cx);
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);

    let stale = binding_authoring(cx, &binding);
    assert_eq!(
        stale.port(port_id()).unwrap().facts().data,
        serde_json::Value::Null,
    );
    runtime
        .execute(GraphCommand::set_port_data(
            runtime.snapshot().unwrap().version(),
            port_id(),
            serde_json::json!("newer graph"),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("direct command advances the graph");
    assert_eq!(
        stale.port(port_id()).unwrap().facts().data,
        serde_json::Value::Null,
        "an owned authoring view does not silently refresh its graph root",
    );
    let fresh = binding_authoring(cx, &binding);
    assert_eq!(
        fresh.port(port_id()).unwrap().facts().data,
        serde_json::json!("newer graph"),
    );
    let stale_version = stale.version();
    let stale_harness = mutation_harness(cx, &binding);
    let stale_value = serde_json::json!("stale");
    let stale_result = cx.update_entity(&stale_harness, |harness, cx| {
        harness.mutation.dispatch(
            stale,
            GraphCommand::set_node_data(stale_version, node_id(), stale_value.clone()),
            MutationRequest::new(stale_value),
            MutationLayoutEffect::Preserve,
            cx,
        )
    });
    assert!(matches!(
        stale_result,
        Err(MutationDispatchError::NotDispatched { .. })
    ));
    draw(cx);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

#[open_gpui::test]
fn host_close_reason_is_preserved_for_binding_and_captured_authoring(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph_with_nodes(1))
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let captured_binding = Rc::new(RefCell::new(None::<Entity<OpenGpuiNodeBinding>>));
    let factory_binding = captured_binding.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        *factory_binding.borrow_mut() = Some(binding.clone());
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
    let binding = captured_binding
        .borrow()
        .clone()
        .expect("captured live binding");
    let captured_authoring = binding_authoring(cx, &binding);
    let mutation = mutation_harness(cx, &binding);

    let outcome = cx
        .update_window_entity(&host, |host, window, cx| host.close_in_window(window, cx))
        .expect("host closes without window-owned interaction");
    assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
    cx.read_entity(&binding, |binding, _| {
        assert!(matches!(
            binding.authoring(),
            Err(OpenGpuiNodeAuthoringError::Closed {
                reason: ViewCloseReason::HostClosed,
            })
        ));
    });

    let captured_version = captured_authoring.version();
    let closed_value = serde_json::json!("closed");
    let closed_result = cx.update_entity(&mutation, |harness, cx| {
        harness.mutation.dispatch(
            captured_authoring,
            GraphCommand::set_node_data(captured_version, node_id(), closed_value.clone()),
            MutationRequest::new(closed_value),
            MutationLayoutEffect::Preserve,
            cx,
        )
    });
    assert!(matches!(
        closed_result,
        Err(MutationDispatchError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[open_gpui::test]
fn pending_mutation_retirement_is_terminal_across_same_instance_remount(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, bindings, attempts, cx) = mount_managed_mutation_scene(cx, true);
    let old = bindings.borrow()[0].clone();
    let old_instance = cx.read_entity(&old, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    let old_activity = cx.read_entity(&old, |binding, _| binding.activity());
    let mut old_mutation = cx.read_entity(&old, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let old_value = serde_json::json!({ "owner": "retired" });

    dispatch_pending_data_mutation(cx, &old, &mut old_mutation, old_value.clone());
    assert_mutation_resources(cx, &host, &old_activity, 2, 1);
    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));

    assert_revoked_activity(&old_activity);
    assert_terminal_retired(&old_mutation, &old_value);
    assert_host_protection(cx, &host, 0, 0);
    let retired_error = cx.read_entity(&old, |binding, _| {
        binding
            .authoring()
            .expect_err("retired binding rejects authoring")
    });
    assert!(matches!(
        old_mutation.observe_authoring_error(&retired_error),
        MutationSettlement::Retired { .. }
    ));

    draw(cx);
    assert_eq!(attempts.get(), 2);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .unwrap()
            .data,
        old_value,
        "journal admission remains host-owned after the old manager retires",
    );
    assert_terminal_retired(&old_mutation, &old_value);
    assert_revoked_activity(&old_activity);

    draw(cx);
    let replacement = bindings.borrow()[1].clone();
    let replacement_instance = cx.read_entity(&replacement, |binding, _| {
        binding.authoring().unwrap().instance().clone()
    });
    assert_eq!(old_instance, replacement_instance);
    let replacement_activity = cx.read_entity(&replacement, |binding, _| binding.activity());
    assert!(!replacement_activity.is_revoked());
    assert_eq!(replacement_activity.active_count(), 0);

    let replacement_value = serde_json::json!({ "owner": "replacement" });
    let mut replacement_mutation = cx.read_entity(&replacement, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    dispatch_pending_data_mutation(
        cx,
        &replacement,
        &mut replacement_mutation,
        replacement_value.clone(),
    );
    assert_mutation_resources(cx, &host, &replacement_activity, 2, 1);
    draw(cx);
    draw(cx);
    let replacement_authoring = binding_authoring(cx, &replacement);
    assert!(matches!(
        cx.update_entity(&replacement, |_, cx| replacement_mutation.observe(
            &replacement_authoring,
            |intent| replacement_authoring.node().data == *intent,
            cx,
        )),
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::NotRequested,
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
    assert_mutation_resources(cx, &host, &replacement_activity, 0, 0);
    assert_terminal_retired(&old_mutation, &old_value);

    let close = cx
        .update_window_entity(&host, |host, window, cx| host.close_in_window(window, cx))
        .expect("settled remount has no journal work left for close");
    assert_eq!(close.command_completions().count(), 0);
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    cx.run_until_parked();
    assert_terminal_retired(&old_mutation, &old_value);
    assert_host_terminal_resources(cx, &host);
}

#[open_gpui::test]
fn pending_mutation_host_close_wins_over_deferred_ticket_resolution(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, bindings, _attempts, cx) = mount_managed_mutation_scene(cx, false);
    let binding = bindings.borrow()[0].clone();
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let value = serde_json::json!({ "owner": "closed" });

    dispatch_pending_data_mutation(cx, &binding, &mut mutation, value.clone());
    assert_mutation_resources(cx, &host, &activity, 2, 1);
    let close = cx
        .update_window_entity(&host, |host, window, cx| host.close_in_window(window, cx))
        .expect("host close drains the pending managed command");

    assert_eq!(close.reason(), ViewCloseReason::HostClosed);
    assert_eq!(close.command_completions().count(), 0);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_revoked_activity(&activity);
    assert_terminal_closed(&mutation, &value);
    assert_host_terminal_resources(cx, &host);

    let closed_error = cx.read_entity(&binding, |binding, _| {
        binding
            .authoring()
            .expect_err("closed binding rejects authoring")
    });
    assert!(matches!(
        mutation.observe_authoring_error(&closed_error),
        MutationSettlement::Closed {
            reason: ViewCloseReason::HostClosed,
            ..
        }
    ));
    cx.run_until_parked();

    assert_terminal_closed(&mutation, &value);
    assert_revoked_activity(&activity);
    assert_host_terminal_resources(cx, &host);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
}

#[open_gpui::test]
fn dropping_pending_mutation_owner_releases_leases_and_host_settles_journal_once(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, bindings, _attempts, cx) = mount_managed_mutation_scene(cx, false);
    let binding = bindings.borrow()[0].clone();
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let mut mutation = cx.read_entity(&binding, |binding, _| {
        binding.managed_mutation::<serde_json::Value>()
    });
    let value = serde_json::json!({ "owner": "dropped" });

    dispatch_pending_data_mutation(cx, &binding, &mut mutation, value.clone());
    assert_mutation_resources(cx, &host, &activity, 2, 1);
    drop(mutation);

    assert!(!activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    assert_eq!(activity.protected_count(), 0);
    assert_host_protection(cx, &host, 0, 0);

    let close = cx
        .update_window_entity(&host, |host, window, cx| host.close_in_window(window, cx))
        .expect("host retains and settles a command after its mutation owner drops");
    assert_eq!(close.command_completions().count(), 0);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .unwrap()
            .data,
        value,
    );
    assert_revoked_activity(&activity);
    assert_host_terminal_resources(cx, &host);

    cx.run_until_parked();
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_revoked_activity(&activity);
    assert_host_terminal_resources(cx, &host);
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
            *factory_handle.borrow_mut() =
                binding.read(cx).authoring().ok().and_then(|authoring| {
                    authoring.port(port_id()).map(|port| port.handle().clone())
                });
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
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
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
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn old_node_drag_handle_is_inert_after_same_instance_retry_and_replacement_drag_succeeds(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph_with_nodes(1))
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let attempts = Rc::new(Cell::new(0_u32));
    let stale_handle = Rc::new(RefCell::new(None::<OpenGpuiNodeDragHandle>));
    let factory_attempts = attempts.clone();
    let factory_handle = stale_handle.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        if factory_handle.borrow().is_none() {
            *factory_handle.borrow_mut() = Some(binding.read(cx).drag_handle());
        }
        if factory_attempts.get() == 1 {
            return Err(OpenGpuiNodeMountError::new("retry node drag fixture"));
        }
        Ok(cx.new(|_| InteractiveDragProbeNode { binding }))
    });

    let captured_host = Rc::new(RefCell::new(None::<Entity<OpenGpuiSceneHost>>));
    let scene_host = captured_host.clone();
    let scene_handle = stale_handle.clone();
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                visible_viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        ExternalDragScene {
            host,
            stale_handle: scene_handle,
        }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.update_entity(&scene, |_, cx| cx.notify());
    draw(cx);

    assert!(cx.update_entity(&host, |host, cx| host.retry_mount(node_id(), cx)));
    draw(cx);
    assert_eq!(attempts.get(), 2);
    assert!(
        !stale_handle
            .borrow()
            .as_ref()
            .expect("captured stale node drag handle")
            .interaction_enabled(),
        "retry must revoke the exact old mount capability",
    );

    let position_before = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&node_id())
        .expect("node before stale drag")
        .pos;
    let history_before = runtime.history_state().unwrap();
    let selection_before = weak_view.selection().unwrap();
    let stale_bounds = cx
        .debug_bounds("stale-mount-drag")
        .expect("external stale drag bounds");
    let stale_start = stale_bounds.center();
    let stale_end = open_gpui::point(stale_start.x - px(50.0), stale_start.y + px(35.0));
    cx.simulate_mouse_down(stale_start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(stale_end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(stale_end, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .expect("node after stale drag")
            .pos,
        position_before,
    );
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(weak_view.selection().unwrap(), selection_before);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
    });

    draw(cx);
    let live_bounds = cx
        .debug_bounds("live-mount-drag")
        .expect("replacement drag bounds");
    let live_start = live_bounds.center();
    let live_end = open_gpui::point(live_start.x + px(50.0), live_start.y + px(35.0));
    cx.simulate_mouse_down(live_start, MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.simulate_mouse_move(live_end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(live_end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    assert_ne!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .expect("node after replacement drag")
            .pos,
        position_before,
        "the replacement handle must move its live mount",
    );
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        history_before.undo_len() + 1,
        "one replacement drag creates one graph history boundary",
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
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
            *factory_handle.borrow_mut() =
                binding.read(cx).authoring().ok().and_then(|authoring| {
                    authoring.port(port_id()).map(|port| port.handle().clone())
                });
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
            .with_navigation_config(
                OpenGpuiNavigationConfig::default().with_input_bindings(
                    OpenGpuiInputBindings::default().with_connect_on_click(false),
                ),
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
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
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
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });

    cx.simulate_mouse_up(live_port.center(), MouseButton::Left, Default::default());
    assert_eq!(
        live_phases.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start, OpenGpuiPortDragPhase::Release,]
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

fn mount_managed_mutation_scene(
    cx: &mut open_gpui::TestAppContext,
    fail_first_mount: bool,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    Rc<RefCell<Vec<Entity<OpenGpuiNodeBinding>>>>,
    Rc<Cell<u32>>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph_with_nodes(1))
            .expect("valid managed-mutation graph"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("managed-mutation view");
    let attempts = Rc::new(Cell::new(0_u32));
    let bindings = Rc::new(RefCell::new(Vec::new()));
    let factory_attempts = Rc::clone(&attempts);
    let factory_bindings = Rc::clone(&bindings);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.try_register(RENDERER, move |binding, _window, cx| {
        factory_attempts.set(factory_attempts.get().saturating_add(1));
        factory_bindings.borrow_mut().push(binding.clone());
        if fail_first_mount && factory_attempts.get() == 1 {
            return Err(OpenGpuiNodeMountError::new(
                "pending mutation retirement fixture",
            ));
        }
        Ok(cx.new(|_| ProbeNode { _binding: binding }))
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid managed-mutation registry"),
            visible_viewport(),
        )
    });
    draw(cx);
    assert_eq!(attempts.get(), 1);
    assert_eq!(bindings.borrow().len(), 1);
    (runtime, host, bindings, attempts, cx)
}

fn dispatch_pending_data_mutation(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
    mutation: &mut OpenGpuiManagedMutation<serde_json::Value>,
    value: serde_json::Value,
) {
    let settlement = cx.update_entity(binding, |binding, cx| {
        let authoring = binding.authoring().expect("live pending authoring");
        let command =
            GraphCommand::set_node_data(authoring.version(), authoring.node_id(), value.clone());
        mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(value),
                MutationLayoutEffect::Preserve,
                cx,
            )
            .expect("pending mutation enters the journal")
    });
    assert_eq!(
        settlement,
        MutationSettlement::Pending {
            awaiting_authority: false,
        }
    );
}

fn assert_mutation_resources(
    cx: &mut open_gpui::VisualTestContext,
    host: &Entity<OpenGpuiSceneHost>,
    activity: &OpenGpuiNodeActivityHandle,
    active: usize,
    protected: usize,
) {
    assert_eq!(activity.active_count(), active);
    assert_eq!(activity.protected_count(), protected);
    assert_host_protection(cx, host, usize::from(protected > 0), protected);
}

fn assert_host_protection(
    cx: &mut open_gpui::VisualTestContext,
    host: &Entity<OpenGpuiSceneHost>,
    instances: usize,
    leases: usize,
) {
    cx.read_entity(host, |host, _| {
        let resources = host.resource_snapshot();
        assert_eq!(resources.protected_instances(), instances);
        assert_eq!(resources.protected_leases(), leases);
    });
}

fn assert_revoked_activity(activity: &OpenGpuiNodeActivityHandle) {
    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    assert_eq!(activity.protected_count(), 0);
}

fn assert_terminal_retired(
    mutation: &OpenGpuiManagedMutation<serde_json::Value>,
    intent: &serde_json::Value,
) {
    assert!(matches!(
        mutation.settlement(),
        MutationSettlement::Retired { .. }
    ));
    assert_eq!(mutation.active_intent(), Some(intent));
    assert!(mutation.retry().is_none());
    assert!(mutation.confirm_uncertain_retry().is_none());
    assert!(!mutation.can_confirm_uncertain_retry());
}

fn assert_terminal_closed(
    mutation: &OpenGpuiManagedMutation<serde_json::Value>,
    intent: &serde_json::Value,
) {
    assert!(matches!(
        mutation.settlement(),
        MutationSettlement::Closed {
            reason: ViewCloseReason::HostClosed,
            ..
        }
    ));
    assert_eq!(mutation.active_intent(), Some(intent));
    assert!(mutation.retry().is_none());
    assert!(mutation.confirm_uncertain_retry().is_none());
    assert!(!mutation.can_confirm_uncertain_retry());
}

fn assert_host_terminal_resources(
    cx: &mut open_gpui::VisualTestContext,
    host: &Entity<OpenGpuiSceneHost>,
) {
    cx.read_entity(host, |host, _| {
        let resources = host.resource_snapshot();
        assert_eq!(resources.active_nodes(), 0);
        assert_eq!(resources.parked_nodes(), 0);
        assert_eq!(resources.strong_instances(), 0);
        assert_eq!(resources.protected_instances(), 0);
        assert_eq!(resources.protected_leases(), 0);
    });
}

fn binding_authoring(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
) -> OpenGpuiNodeAuthoring {
    cx.read_entity(binding, |binding, _| binding.authoring().unwrap())
}

fn mutation_harness(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
) -> Entity<MutationHarness> {
    cx.update(|_, cx| {
        let mutation = binding.read(cx).managed_mutation();
        cx.new(|_| MutationHarness { mutation })
    })
}

fn assert_revoked_binding(
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
) {
    cx.read_entity(binding, |binding, _| {
        assert!(matches!(
            binding.authoring(),
            Err(OpenGpuiNodeAuthoringError::RetiredMount { node }) if node == node_id()
        ));
        assert!(binding.activity().is_revoked());
    });
    assert!(
        !cx.update_entity(binding, |binding, cx| binding.invalidate_layout(cx))
            .expect("retired layout invalidation is a benign no-op")
    );
}

fn assert_stale_authoring_rejected(
    stale_execute: OpenGpuiNodeAuthoring,
    stale_facts: OpenGpuiNodeAuthoring,
    binding: &Entity<OpenGpuiNodeBinding>,
    mutation: &Entity<MutationHarness>,
    cx: &mut open_gpui::VisualTestContext,
) {
    let value = serde_json::json!("retired authoring must not execute");
    let command = GraphCommand::set_node_data(stale_execute.version(), node_id(), value.clone());
    let stale_result = cx.update_entity(mutation, |harness, cx| {
        harness.mutation.dispatch(
            stale_execute,
            command,
            MutationRequest::new(value),
            MutationLayoutEffect::Preserve,
            cx,
        )
    });
    assert!(matches!(stale_result, Err(MutationDispatchError::Retired)));
    let stale_port = stale_facts
        .port(port_id())
        .expect("owned authoring preserves coherent old facts");
    assert_eq!(stale_port.facts().node, node_id());
    assert_eq!(stale_port.handle().port(), port_id());
    cx.read_entity(binding, |binding, _| assert!(binding.authoring().is_err()));
}

fn assert_live_replacement(
    runtime: &GraphRuntime,
    cx: &mut open_gpui::VisualTestContext,
    binding: &Entity<OpenGpuiNodeBinding>,
    value: serde_json::Value,
) {
    cx.read_entity(binding, |binding, _| {
        assert!(binding.authoring().unwrap().port(port_id()).is_some());
        assert!(!binding.activity().is_revoked());
    });
    let mutation = mutation_harness(cx, binding);
    let authoring = binding_authoring(cx, binding);
    let command = GraphCommand::set_node_data(authoring.version(), node_id(), value.clone());
    cx.update_entity(&mutation, |harness, cx| {
        harness
            .mutation
            .dispatch(
                authoring,
                command,
                MutationRequest::new(value.clone()),
                MutationLayoutEffect::InvalidateWhenSatisfied,
                cx,
            )
            .expect("replacement binding admits managed mutation");
    });
    draw(cx);
    let fresh = binding_authoring(cx, binding);
    let settlement = cx.update_entity(&mutation, |harness, cx| {
        harness
            .mutation
            .observe(&fresh, |intent| fresh.node().data == *intent, cx)
    });
    assert!(matches!(
        settlement,
        MutationSettlement::Satisfied {
            layout: MutationLayoutSettlement::Invalidated,
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn graph_with_nodes(count: usize) -> jellyflow_core::Graph {
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
