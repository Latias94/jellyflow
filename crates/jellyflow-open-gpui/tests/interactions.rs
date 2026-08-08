use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::interaction::{
    OpenGpuiPortActivationSource, OpenGpuiPortDragCancelReason, OpenGpuiPortDragPhase,
    OpenGpuiPortEvent,
};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
    commands::TopologyPort, graph::GraphRuntime, interaction::CanvasInteractionPhase,
    view::RendererStamp, view::ViewCloseReason, view::ViewSessionOptions, view::WeakViewSession,
};
use open_gpui::{
    AppContext, Context, Entity, KeyUpEvent, Keystroke, Modifiers, MouseButton, ParentElement,
    Render, ScrollDelta, ScrollWheelEvent, Styled, TouchPhase, WeakFocusHandle, Window, div, point,
    prelude::*, px,
};

const KIND: &str = "test.interactive-node";
const RENDERER: &str = "test.native.interactive-node";
const PORT_SELECTOR: &str = "jellyflow-test-interactive-port";
const CONTROL_SELECTOR: &str = "jellyflow-test-native-control";
const SCROLL_SELECTOR: &str = "jellyflow-test-native-scroll";

struct InteractiveNode {
    binding: Entity<OpenGpuiNodeBinding>,
    control_focus: open_gpui::FocusHandle,
    port: Rc<PortProbe>,
    observer_host: Rc<RefCell<Option<Entity<OpenGpuiSceneHost>>>>,
    observe_port: bool,
    node_events: Rc<RefCell<Vec<&'static str>>>,
    control_events: Rc<RefCell<Vec<&'static str>>>,
    scroll_events: Rc<RefCell<Vec<&'static str>>>,
}

#[derive(Default)]
struct PortProbe {
    pointer: RefCell<Vec<OpenGpuiPortDragPhase>>,
    modifiers: RefCell<Vec<Modifiers>>,
    activations: RefCell<Vec<OpenGpuiPortActivationSource>>,
    host_phases: RefCell<Vec<CanvasInteractionPhase>>,
    enabled: Cell<bool>,
}

impl PortProbe {
    fn enabled() -> Self {
        Self {
            enabled: Cell::new(true),
            ..Self::default()
        }
    }
}

struct InteractionScene {
    host: Entity<OpenGpuiSceneHost>,
    canvas_events: Rc<RefCell<Vec<&'static str>>>,
}

impl Render for InteractiveNode {
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
            .and_then(|authoring| {
                authoring.port(output_id()).map(|port| {
                    let handle = port.handle().clone();
                    handle
                        .with_interaction_enabled(self.port.enabled.get())
                        .with_minimum_hit_diameter(22.0)
                })
            });
        let port = self.port.clone();
        let observer_host = self.observer_host.clone();
        let down_events = self.node_events.clone();
        let move_events = self.node_events.clone();
        let up_events = self.node_events.clone();
        let node_scroll_events = self.node_events.clone();
        let control_down_events = self.control_events.clone();
        let control_up_events = self.control_events.clone();
        let control_key_events = self.control_events.clone();
        let control_focus = self.control_focus.clone();
        let focus_on_mouse_down = control_focus.clone();
        let scroll_events = self.scroll_events.clone();

        div()
            .relative()
            .size_full()
            .on_any_mouse_down(move |_, _, _| down_events.borrow_mut().push("down"))
            .on_mouse_move(move |event, _, _| {
                if event.pressed_button.is_some() {
                    move_events.borrow_mut().push("move");
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                let mut events = up_events.borrow_mut();
                let downs = events.iter().filter(|event| **event == "down").count();
                let ups = events.iter().filter(|event| **event == "up").count();
                if downs > ups {
                    events.push("up");
                }
            })
            .on_scroll_wheel(move |_, _, _| node_scroll_events.borrow_mut().push("scroll"))
            .child(
                div().absolute().left(px(12.0)).top(px(12.0)).child(
                    div()
                        .debug_selector(|| CONTROL_SELECTOR.to_owned())
                        .track_focus(&control_focus)
                        .w(px(72.0))
                        .h(px(24.0))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            window.focus(&focus_on_mouse_down, cx);
                            control_down_events.borrow_mut().push("down");
                        })
                        .on_mouse_up(MouseButton::Left, move |_, _, _| {
                            control_up_events.borrow_mut().push("up")
                        })
                        .on_key_down(move |event, _, cx| {
                            if event.keystroke.key.eq_ignore_ascii_case("tab")
                                && control_key_events.borrow().contains(&"consume-tab")
                            {
                                control_key_events.borrow_mut().push("tab");
                                cx.stop_propagation();
                            }
                        }),
                ),
            )
            .child(
                div().absolute().left(px(12.0)).top(px(52.0)).child(
                    div()
                        .debug_selector(|| SCROLL_SELECTOR.to_owned())
                        .w(px(72.0))
                        .h(px(24.0))
                        .on_scroll_wheel(move |_, window, _| {
                            scroll_events.borrow_mut().push("scroll");
                            window.prevent_default();
                        }),
                ),
            )
            .children(handle.map(|handle| {
                let marker = div()
                    .debug_selector(|| PORT_SELECTOR.to_owned())
                    .w(px(14.0))
                    .h(px(14.0));
                let interactive = if self.observe_port {
                    handle.interactive_with_observer(marker, move |event, _, cx| {
                        let host = observer_host
                            .borrow()
                            .clone()
                            .expect("observer host is mounted before node render");
                        port.host_phases
                            .borrow_mut()
                            .push(host.read(cx).interaction_presentation().phase());
                        match event {
                            OpenGpuiPortEvent::Pointer(event) => {
                                port.pointer.borrow_mut().push(event.phase());
                                port.modifiers.borrow_mut().push(event.modifiers());
                            }
                            OpenGpuiPortEvent::Activation(event) => {
                                port.activations.borrow_mut().push(event.source());
                            }
                        }
                    })
                } else {
                    handle.interactive(marker)
                };
                div()
                    .absolute()
                    .right(px(8.0))
                    .top(px(43.0))
                    .child(interactive)
                    .into_any_element()
            }))
    }
}

impl Render for InteractionScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let down_events = self.canvas_events.clone();
        let move_events = self.canvas_events.clone();
        let up_events = self.canvas_events.clone();
        let scroll_events = self.canvas_events.clone();
        div()
            .size_full()
            .on_key_down(|_, _, _| {})
            .on_any_mouse_down(move |_, _, _| down_events.borrow_mut().push("down"))
            .on_mouse_move(move |event, _, _| {
                if event.pressed_button.is_some() {
                    move_events.borrow_mut().push("move");
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                let mut events = up_events.borrow_mut();
                let downs = events.iter().filter(|event| **event == "down").count();
                let ups = events.iter().filter(|event| **event == "up").count();
                if downs > ups {
                    events.push("up");
                }
            })
            .on_scroll_wheel(move |_, _, _| scroll_events.borrow_mut().push("scroll"))
            .child(self.host.clone())
    }
}

#[open_gpui::test]
fn tab_shift_tab_enter_and_space_use_native_keyboard_semantics_without_pointer_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, _host, cx, port, node_events, canvas_events, _, _, control_focus, _) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");

    cx.simulate_keystrokes("tab");
    let port_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("port must be the next tab stop");
    assert_ne!(port_focus, control_focus);

    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus.clone())));
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(port_focus.clone())));

    simulate_key_press(cx, "enter");
    simulate_key_press(cx, "space");
    simulate_key_press(cx, "shift-enter");
    cx.run_until_parked();

    assert_eq!(
        port.activations.borrow().as_slice(),
        [
            OpenGpuiPortActivationSource::Keyboard,
            OpenGpuiPortActivationSource::Keyboard,
        ]
    );
    assert!(port.pointer.borrow().is_empty());
    assert!(node_events.borrow().is_empty());
    assert!(
        canvas_events.borrow().is_empty(),
        "outer app events: {:?}",
        canvas_events.borrow().as_slice(),
    );
}

#[open_gpui::test]
fn native_control_can_consume_tab_before_graph_focus_fallback(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx, port, node_events, canvas_events, control_events, _, _, _) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    let focused = cx.update(|window, cx| window.focused(cx));
    node_events.borrow_mut().clear();
    control_events.borrow_mut().clear();
    control_events.borrow_mut().push("consume-tab");

    cx.simulate_keystrokes("tab");

    assert_eq!(cx.update(|window, cx| window.focused(cx)), focused);
    assert_eq!(control_events.borrow().as_slice(), ["consume-tab", "tab"]);
    assert!(port.activations.borrow().is_empty());
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn semantic_only_port_keeps_keyboard_and_pointer_connection_behavior_without_an_observer(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, node_events, _, _, _, _, _) =
        mount_scene_with_port_observer(cx, false);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    cx.simulate_keystrokes("tab");
    simulate_key_press(cx, "enter");
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
        assert!(host.connection_search_request().is_none());
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
    });

    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch,
        );
        assert!(host.connection_search_request().is_some());
    });
    assert!(port_events.pointer.borrow().is_empty());
    assert!(port_events.activations.borrow().is_empty());
    assert!(port_events.host_phases.borrow().is_empty());
}

#[open_gpui::test]
fn rejected_pointer_start_reports_start_before_cancel_without_cancelling_keyboard_connection(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port, _, _, _, _, _, _) = mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    cx.simulate_keystrokes("tab");
    simulate_key_press(cx, "enter");
    cx.run_until_parked();
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert!(host.connection_search_request().is_none());
    });

    let bounds = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert_eq!(
        port.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::InteractionUnavailable),
        ],
        "a deferred rejection must not overtake the product's Start callback",
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.connection_search_request().is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[open_gpui::test]
fn disabled_port_is_skipped_while_pointer_input_passes_through_to_the_node(
    cx: &mut open_gpui::TestAppContext,
) {
    let (
        _runtime,
        host,
        cx,
        port_events,
        node_events,
        canvas_events,
        control_events,
        _,
        control_focus,
        view,
    ) = mount_scene(cx);
    draw(cx);

    let enabled_geometry = view
        .geometry_snapshot()
        .expect("enabled port geometry snapshot")
        .port(output_id())
        .expect("enabled port geometry")
        .clone();

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    cx.simulate_keystrokes("tab");
    let enabled_port_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("enabled port focus");

    port_events.enabled.set(false);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    let disabled_geometry = view
        .geometry_snapshot()
        .expect("disabled port geometry snapshot")
        .port(output_id())
        .expect("disabled port keeps published geometry")
        .clone();
    assert_eq!(
        disabled_geometry.visual_bounds(),
        enabled_geometry.visual_bounds()
    );
    assert_eq!(
        disabled_geometry.route_anchor(),
        enabled_geometry.route_anchor()
    );

    simulate_key_press(cx, "enter");
    simulate_key_press(cx, "space");
    cx.run_until_parked();
    assert!(port_events.activations.borrow().is_empty());

    cx.simulate_keystrokes("shift-tab");
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus.clone())));
    cx.simulate_keystrokes("tab");
    let next_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("node focus after disabled port is skipped");
    assert_ne!(next_focus, control_focus);
    assert_ne!(next_focus, enabled_port_focus);

    control_events.borrow_mut().clear();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_click(port.center(), Default::default());

    assert!(port_events.pointer.borrow().is_empty());
    assert!(port_events.activations.borrow().is_empty());
    assert!(control_events.borrow().is_empty());
    assert_eq!(node_events.borrow().as_slice(), ["down", "up"]);
    assert!(canvas_events.borrow().is_empty());

    port_events.enabled.set(true);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start, OpenGpuiPortDragPhase::Release]
    );
}

#[open_gpui::test]
fn disabling_an_active_port_drag_cancels_only_that_owner_and_releases_resources(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus, _) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    port_events.enabled.set(false);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::PortDisabled),
        ]
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    assert_eq!(port_events.pointer.borrow().len(), 2);
}

#[open_gpui::test]
fn removing_a_captured_dynamic_port_cancels_capture_after_the_topology_patch(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _, _) = mount_scene(cx);
    draw(cx);

    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("dynamic-port removal snapshot")
                .version(),
            TopologyEdit::new().replace_node_ports(
                node_id(),
                Vec::<TopologyPort>::new(),
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .expect("remove dynamic port")
        .applied()
        .expect("dynamic port removal applies");
    cx.run_until_parked();
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::PortDisabled),
        ]
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.connection_search_request().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    assert!(
        runtime
            .snapshot()
            .expect("updated graph")
            .graph()
            .port(&output_id())
            .is_none()
    );

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    assert_eq!(port_events.pointer.borrow().len(), 2);
}

#[open_gpui::test]
fn removing_a_keyboard_activated_dynamic_port_cancels_keyboard_connection(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus, _) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));
    cx.simulate_keystrokes("tab");
    simulate_key_press(cx, "enter");
    cx.run_until_parked();
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert!(host.connection_search_request().is_none());
    });

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("keyboard-port removal snapshot")
                .version(),
            TopologyEdit::new().replace_node_ports(
                node_id(),
                Vec::<TopologyPort>::new(),
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .expect("remove dynamic port")
        .applied()
        .expect("dynamic port removal applies");
    cx.run_until_parked();
    draw(cx);

    assert_eq!(
        port_events.activations.borrow().as_slice(),
        [OpenGpuiPortActivationSource::Keyboard]
    );
    assert!(port_events.pointer.borrow().is_empty());
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.connection_search_request().is_none());
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn disabling_a_port_does_not_steal_focus_acquired_after_the_drag_started(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, _, _, _, _, control_focus, _) = mount_scene(cx);
    draw(cx);

    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    let new_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| window.focus(&new_focus, cx));

    port_events.enabled.set(false);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::PortDisabled),
        ]
    );
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(new_focus.clone())));
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn port_capture_survives_redraw_and_transfers_out_of_bounds_release_to_search(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus, _) =
        mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .expect("control focus")
        .clone();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    let modifiers = Modifiers::shift() | Modifiers::secondary_key();
    let graph_before = runtime.snapshot().expect("snapshot before port capture");
    let history_before = runtime
        .history_state()
        .expect("history before port capture");

    cx.simulate_mouse_down(port.center(), MouseButton::Left, modifiers);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start]
    );
    assert_eq!(
        port_events.host_phases.borrow().as_slice(),
        [CanvasInteractionPhase::Armed],
        "the host reducer must run before the application observer, exactly once",
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    let first_hitbox = cx
        .update(|window, _| window.captured_hitbox())
        .expect("port captured pointer");
    cx.read_entity(&host, |host, _| {
        let owner = host.active_port_drag().expect("active port owner");
        assert_eq!(owner.instance().node(), node_id());
        assert_eq!(owner.port(), output_id());
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });

    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    let rebound_hitbox = cx
        .update(|window, _| window.captured_hitbox())
        .expect("capture rebound after redraw");
    assert_ne!(rebound_hitbox, first_hitbox);

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), modifiers);
    cx.simulate_mouse_up(outside, MouseButton::Left, modifiers);
    cx.run_until_parked();

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Move,
            OpenGpuiPortDragPhase::Release,
        ],
    );
    assert_eq!(
        port_events.modifiers.borrow().as_slice(),
        [modifiers, modifiers, modifiers],
        "native port ownership preserves modifiers through start, move, and release",
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| {
        assert_ne!(window.focused(cx), control_focus.upgrade());
        assert!(window.focused(cx).is_some());
    });
    let request = cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        host.connection_search_request().expect("search request")
    });
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.cancel_connection_search(&request, window, cx)
        })
        .expect("cancel search");
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), control_focus.upgrade());
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after port capture")));
    assert_eq!(
        runtime.history_state().expect("history after port capture"),
        history_before,
    );
}

#[open_gpui::test]
fn escape_cancels_port_capture_and_consumes_the_key(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus, _) =
        mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .expect("control focus")
        .clone();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
        ],
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), control_focus.upgrade());
    });
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn hard_retirement_cancels_capture_and_suppresses_late_pointer_events(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let previous_focus_handle = cx.update(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        focus
    });
    let previous_focus = previous_focus_handle.downgrade();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    drop(previous_focus_handle);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("hard-retirement snapshot")
                .version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
    assert!(previous_focus.upgrade().is_none());

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start],
        "retired mounts must not invoke application observers",
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
}

#[open_gpui::test]
fn hard_retirement_does_not_steal_focus_acquired_after_drag_started(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, _, _, _, _, control_focus, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    let new_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| {
        window.focus(&new_focus, cx);
        assert_eq!(window.focused(cx), Some(new_focus.clone()));
    });
    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("focus-retirement snapshot")
                .version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    cx.run_until_parked();
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start],
        "retired mounts must not invoke application observers",
    );
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_none()));
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), Some(new_focus.clone()));
    });
}

#[open_gpui::test]
fn native_control_and_scroll_keep_component_bubbling_without_graph_gestures(
    cx: &mut open_gpui::TestAppContext,
) {
    let (
        runtime,
        host,
        cx,
        _port_events,
        node_events,
        canvas_events,
        control_events,
        scroll_events,
        _,
        view,
    ) = mount_scene(cx);
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before native controls");
    let history_before = runtime
        .history_state()
        .expect("history before native controls");
    let viewport_before = view
        .viewport_transform()
        .expect("viewport before native controls");
    let selection_before = view.selection().expect("selection before native controls");

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(control_events.borrow().as_slice(), ["down", "up"]);
    assert_eq!(node_events.borrow().as_slice(), ["down", "up"]);
    assert!(canvas_events.borrow().is_empty());

    let scroll = cx.debug_bounds(SCROLL_SELECTOR).expect("scroll bounds");
    cx.simulate_event(ScrollWheelEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -1.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    assert_eq!(scroll_events.borrow().as_slice(), ["scroll"]);
    assert_eq!(node_events.borrow().as_slice(), ["down", "up", "scroll"]);
    assert!(canvas_events.borrow().is_empty());
    assert!(
        graph_before.ptr_eq(&runtime.snapshot().expect("graph after native controls")),
        "native controls must not create graph history or replace the graph root",
    );
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(view.viewport_transform().unwrap(), viewport_before);
    assert_eq!(view.selection().unwrap(), selection_before);
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn captured_owner_is_force_included_until_release_then_becomes_parkable(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert!(host.active_port_drag().is_some());
    });

    cx.simulate_keystrokes("escape");
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
        ],
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert!(host.active_port_drag().is_none());
    });
}

#[open_gpui::test]
fn create_from_wire_search_force_includes_its_source_until_cancelled(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, _, _, _, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let request = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
        host.connection_search_request().expect("search request")
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.connection_search_request().as_ref(), Some(&request));
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.cancel_connection_search(&request, window, cx)
        })
        .expect("cancel search");
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn window_aware_close_releases_capture_before_returning(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    node_events.borrow_mut().clear();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("host close");
        assert!(window.captured_hitbox().is_none());
        assert!(window.focused(cx).is_none());
    });

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start],
        "closed mounts must not invoke application observers",
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn runtime_drop_during_connection_drag_releases_capture_and_terminalizes_the_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start],
        "closed mounts must not invoke application observers",
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn cancelling_create_from_wire_restores_the_pre_drag_focus_and_leaves_no_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, _, _, _, _, _, control_focus, _) = mount_scene(cx);
    draw(cx);
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus.clone())));

    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let request = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        host.connection_search_request()
            .expect("create-from-wire request")
    });
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.cancel_connection_search(&request, window, cx)
        })
        .expect("cancel current search");
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.connection_search_request().is_none());
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));

    let after = runtime.snapshot().unwrap();
    assert!(before.ptr_eq(&after));
    assert_eq!(after.revision(), before.revision());
    assert_eq!(runtime.history_state().unwrap(), history_before);
}

#[open_gpui::test]
fn escape_without_the_original_mouse_up_does_not_suppress_the_next_click(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, _host, cx, port_events, node_events, canvas_events, control_events, _, _, _) =
        mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_keystrokes("escape");

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    cx.run_until_parked();

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
        ],
    );
    assert_eq!(control_events.borrow().as_slice(), ["down", "up"]);
    assert_eq!(node_events.borrow().as_slice(), ["down", "up"]);
    assert!(canvas_events.borrow().is_empty());
}

fn mount_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
    Rc<PortProbe>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Option<WeakFocusHandle>>>,
    WeakViewSession,
) {
    mount_scene_with_port_observer(cx, true)
}

fn mount_scene_with_port_observer(
    cx: &mut open_gpui::TestAppContext,
    observe_port: bool,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
    Rc<PortProbe>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Vec<&'static str>>>,
    Rc<RefCell<Option<WeakFocusHandle>>>,
    WeakViewSession,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let view_handle = view.downgrade();
    let port_events = Rc::new(PortProbe::enabled());
    let node_events = Rc::new(RefCell::new(Vec::new()));
    let canvas_events = Rc::new(RefCell::new(Vec::new()));
    let control_events = Rc::new(RefCell::new(Vec::new()));
    let scroll_events = Rc::new(RefCell::new(Vec::new()));
    let control_focus = Rc::new(RefCell::new(None));
    let captured_host = Rc::new(RefCell::new(None));
    let factory_port_events = port_events.clone();
    let factory_node_events = node_events.clone();
    let factory_control_events = control_events.clone();
    let factory_scroll_events = scroll_events.clone();
    let factory_control_focus = control_focus.clone();
    let factory_observer_host = captured_host.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Interactive node"),
            move |binding, _window, cx| {
                let control_focus = cx.focus_handle().tab_stop(true);
                *factory_control_focus.borrow_mut() = Some(control_focus.downgrade());
                Ok(cx.new(|_| InteractiveNode {
                    binding,
                    control_focus,
                    port: factory_port_events.clone(),
                    observer_host: factory_observer_host.clone(),
                    observe_port,
                    node_events: factory_node_events.clone(),
                    control_events: factory_control_events.clone(),
                    scroll_events: factory_scroll_events.clone(),
                }))
            },
        ))
        .expect("register interactive node renderer");

    let scene_host = captured_host.clone();
    let scene_canvas_events = canvas_events.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractionScene {
            host,
            canvas_events: scene_canvas_events,
        }
    });
    let host = captured_host.borrow().clone().expect("captured scene host");
    (
        runtime,
        host,
        cx,
        port_events,
        node_events,
        canvas_events,
        control_events,
        scroll_events,
        control_focus,
        view_handle,
    )
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn simulate_key_up(cx: &mut open_gpui::VisualTestContext, key: &str) {
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key)
            .expect("valid test keystroke")
            .with_simulated_ime(),
    });
}

fn simulate_key_press(cx: &mut open_gpui::VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    simulate_key_up(cx, key);
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
        ports: vec![output_id()],
        data: serde_json::json!({ "label": "interactive" }),
    };
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node_id(), node);
    graph.insert_port(
        output_id(),
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
        },
    );
    graph.build().unwrap()
}

fn node_id() -> NodeId {
    NodeId::from_u128(10)
}

fn output_id() -> PortId {
    PortId::from_u128(20)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
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
