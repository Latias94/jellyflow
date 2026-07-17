use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{
    FrameRequest, GraphCommand, GraphRuntime, IncidentEdgePolicy, RendererStamp, TopologyEdit,
    TopologyPort, ViewCloseReason, ViewSessionOptions,
};
use jellyflow_open_gpui::{
    OpenGpuiConnectionCancelReason, OpenGpuiConnectionPhase, OpenGpuiNodeBinding,
    OpenGpuiNodeInteractionGuard, OpenGpuiNodeRegistry, OpenGpuiPortActivationSource,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragPhase, OpenGpuiPortEvent, OpenGpuiSceneHost,
    open_gpui_port_element_id,
};
use open_gpui::{
    AppContext, Context, Entity, EntityId, KeyUpEvent, Keystroke, Modifiers, MouseButton,
    ParentElement, Render, ScrollDelta, ScrollWheelEvent, Styled, TouchPhase, WeakFocusHandle,
    Window, div, point, prelude::*, px,
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
    node_events: Rc<RefCell<Vec<&'static str>>>,
    control_events: Rc<RefCell<Vec<&'static str>>>,
    scroll_events: Rc<RefCell<Vec<&'static str>>>,
}

#[derive(Default)]
struct PortProbe {
    pointer: RefCell<Vec<OpenGpuiPortDragPhase>>,
    modifiers: RefCell<Vec<Modifiers>>,
    activations: RefCell<Vec<OpenGpuiPortActivationSource>>,
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
            .port_handle(output_id())
            .map(|handle| {
                handle
                    .with_interaction_enabled(self.port.enabled.get())
                    .with_hit_slop(10.0)
            });
        let port = self.port.clone();
        let down_events = self.node_events.clone();
        let move_events = self.node_events.clone();
        let up_events = self.node_events.clone();
        let node_scroll_events = self.node_events.clone();
        let control_down_events = self.control_events.clone();
        let control_up_events = self.control_events.clone();
        let control_focus = self.control_focus.clone();
        let focus_on_mouse_down = control_focus.clone();
        let scroll_events = self.scroll_events.clone();

        div()
            .relative()
            .size_full()
            .on_any_mouse_down(move |_, _, _| down_events.borrow_mut().push("down"))
            .on_mouse_move(move |_, _, _| move_events.borrow_mut().push("move"))
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                up_events.borrow_mut().push("up")
            })
            .on_scroll_wheel(move |_, _, _| node_scroll_events.borrow_mut().push("scroll"))
            .child(
                div().absolute().left(px(12.0)).top(px(12.0)).child(
                    OpenGpuiNodeInteractionGuard::control(
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
                            }),
                    ),
                ),
            )
            .child(
                div().absolute().left(px(12.0)).top(px(52.0)).child(
                    OpenGpuiNodeInteractionGuard::scroll(
                        div()
                            .debug_selector(|| SCROLL_SELECTOR.to_owned())
                            .w(px(72.0))
                            .h(px(24.0))
                            .on_scroll_wheel(move |_, _, _| {
                                scroll_events.borrow_mut().push("scroll")
                            }),
                    ),
                ),
            )
            .children(handle.map(|handle| {
                div()
                    .absolute()
                    .right(px(8.0))
                    .top(px(43.0))
                    .child(
                        handle.interactive(
                            div()
                                .debug_selector(|| PORT_SELECTOR.to_owned())
                                .w(px(14.0))
                                .h(px(14.0)),
                            move |event, _, _| match event {
                                OpenGpuiPortEvent::Pointer(event) => {
                                    port.pointer.borrow_mut().push(event.phase());
                                    port.modifiers.borrow_mut().push(event.modifiers());
                                }
                                OpenGpuiPortEvent::Activation(event) => {
                                    port.activations.borrow_mut().push(event.source());
                                }
                            },
                        ),
                    )
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
            .on_any_mouse_down(move |_, _, _| down_events.borrow_mut().push("down"))
            .on_mouse_move(move |_, _, _| move_events.borrow_mut().push("move"))
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                up_events.borrow_mut().push("up")
            })
            .on_scroll_wheel(move |_, _, _| scroll_events.borrow_mut().push("scroll"))
            .child(self.host.clone())
    }
}

#[open_gpui::test]
fn tab_shift_tab_enter_and_space_use_native_keyboard_semantics_without_pointer_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, _host, cx, port, node_events, canvas_events, _, _, control_focus) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
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

    simulate_key_up(cx, "enter");
    simulate_key_up(cx, "space");
    simulate_key_up(cx, "shift-enter");

    assert_eq!(
        port.activations.borrow().as_slice(),
        [
            OpenGpuiPortActivationSource::Keyboard,
            OpenGpuiPortActivationSource::Keyboard,
        ]
    );
    assert!(port.pointer.borrow().is_empty());
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
}

#[open_gpui::test]
fn disabled_port_is_skipped_and_blocks_pointer_and_keyboard_activation(
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
    ) = mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    cx.simulate_keystrokes("tab");
    let enabled_port_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("enabled port focus");

    port_events.enabled.set(false);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);

    simulate_key_up(cx, "enter");
    simulate_key_up(cx, "space");
    assert!(port_events.activations.borrow().is_empty());

    cx.simulate_keystrokes("shift-tab");
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus.clone())));
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), Some(control_focus.clone()));
        assert_ne!(window.focused(cx), Some(enabled_port_focus.clone()));
    });

    control_events.borrow_mut().clear();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_click(port.center(), Default::default());

    assert!(port_events.pointer.borrow().is_empty());
    assert!(port_events.activations.borrow().is_empty());
    assert!(control_events.borrow().is_empty());
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());

    port_events.enabled.set(true);
    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(port.center(), MouseButton::Left, Default::default());
    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [OpenGpuiPortDragPhase::Start, OpenGpuiPortDragPhase::Release]
    );
}

#[open_gpui::test]
fn disabling_an_active_port_drag_cancels_only_that_owner_and_releases_resources(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
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
    cx.update(|window, cx| assert!(window.focused(cx).is_none()));
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
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
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
    draw(cx);

    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(
                node_id(),
                Vec::<TopologyPort>::new(),
                IncidentEdgePolicy::Cascade,
            ),
        ))
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
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
        assert!(host.create_from_wire_request().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
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
fn removing_a_keyboard_activated_dynamic_port_cancels_create_from_wire(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus) =
        mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .and_then(WeakFocusHandle::upgrade)
        .expect("mounted control focus");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));
    cx.simulate_keystrokes("tab");
    simulate_key_up(cx, "enter");
    cx.run_until_parked();
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Searching);
        assert!(host.create_from_wire_request().is_some());
    });

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(
                node_id(),
                Vec::<TopologyPort>::new(),
                IncidentEdgePolicy::Cascade,
            ),
        ))
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
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
        assert!(host.create_from_wire_request().is_none());
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn disabling_a_port_does_not_steal_focus_acquired_after_the_drag_started(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, _, _, _, _, control_focus) = mount_scene(cx);
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
        assert_eq!(host.stats().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn port_capture_survives_redraw_and_transfers_out_of_bounds_release_to_search(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus) =
        mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
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
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    let first_hitbox = cx
        .update(|window, _| window.captured_hitbox())
        .expect("port captured pointer");
    cx.read_entity(&host, |host, _| {
        let owner = host.active_port_drag().expect("active port owner");
        assert_eq!(owner.instance().node(), node_id());
        assert_eq!(owner.port(), output_id());
        assert_eq!(host.stats().protected_instances(), 1);
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
        assert_eq!(window.focused(cx), control_focus.upgrade());
    });
    let request = cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Searching);
        assert_eq!(host.stats().protected_instances(), 1);
        host.create_from_wire_request().expect("search request")
    });
    cx.update_entity(&host, |host, _| {
        host.cancel_create_from_wire(request, OpenGpuiConnectionCancelReason::OutsideClick)
            .expect("cancel search");
        assert_eq!(host.stats().protected_instances(), 0);
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after port capture")));
    assert_eq!(
        runtime.history_state().expect("history after port capture"),
        history_before,
    );
}

#[open_gpui::test]
fn escape_cancels_port_capture_and_consumes_the_key(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, control_focus) =
        mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    let control_focus = control_focus
        .borrow()
        .as_ref()
        .expect("control focus")
        .clone();
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    cx.simulate_keystrokes("escape");

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
        assert_eq!(host.stats().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn hard_retirement_cancels_capture_and_suppresses_late_pointer_events(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
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
            0,
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });
    assert!(previous_focus.upgrade().is_none());

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
        ],
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| {
        assert!(window.focused(cx).is_some());
    });
}

#[open_gpui::test]
fn hard_retirement_does_not_steal_focus_acquired_after_drag_started(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, _, _, _, _, control_focus) = mount_scene(cx);
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
            0,
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    cx.run_until_parked();
    draw(cx);

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
        ]
    );
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_none()));
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), Some(new_focus.clone()));
    });
}

#[open_gpui::test]
fn native_control_and_scroll_guards_stop_node_and_canvas_gestures(
    cx: &mut open_gpui::TestAppContext,
) {
    let (
        _runtime,
        _host,
        cx,
        _port_events,
        node_events,
        canvas_events,
        control_events,
        scroll_events,
        _,
    ) = mount_scene(cx);
    draw(cx);

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
    assert_eq!(control_events.borrow().as_slice(), ["down", "up"]);
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());

    let scroll = cx.debug_bounds(SCROLL_SELECTOR).expect("scroll bounds");
    cx.simulate_event(ScrollWheelEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -1.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    assert_eq!(scroll_events.borrow().as_slice(), ["scroll"]);
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
}

#[open_gpui::test]
fn captured_owner_is_force_included_until_release_then_becomes_parkable(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().protected_instances(), 1);
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
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 1);
        assert_eq!(host.stats().protected_instances(), 0);
        assert!(host.active_port_drag().is_none());
    });
}

#[open_gpui::test]
fn create_from_wire_search_force_includes_its_source_until_cancelled(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx, _, _, _, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    let request = cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Searching);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
        host.create_from_wire_request().expect("search request")
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.create_from_wire_request(), Some(request));
    });

    cx.update_entity(&host, |host, cx| {
        host.cancel_create_from_wire(request, OpenGpuiConnectionCancelReason::OutsideClick)
            .expect("cancel search");
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 1);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn window_aware_close_releases_capture_before_returning(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
    draw(cx);
    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());
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
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::HostClosed),
        ],
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn runtime_drop_during_connection_drag_releases_capture_and_terminalizes_the_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, port_events, node_events, canvas_events, _, _, _) = mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::HostClosed),
        ]
    );
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn cancelling_create_from_wire_restores_the_pre_drag_focus_and_leaves_no_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx, _, _, _, _, _, control_focus) = mount_scene(cx);
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
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    cx.update_entity(&host, |host, _| {
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Searching);
        let request = host
            .create_from_wire_request()
            .expect("create-from-wire request");
        host.cancel_create_from_wire(request, OpenGpuiConnectionCancelReason::OutsideClick)
            .expect("cancel current search");
        assert_eq!(host.connection_phase(), OpenGpuiConnectionPhase::Idle);
        assert!(host.create_from_wire_request().is_none());
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
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
    let (_runtime, _host, cx, port_events, node_events, canvas_events, control_events, _, _) =
        mount_scene(cx);
    draw(cx);
    let port = cx.debug_bounds(PORT_SELECTOR).expect("port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    cx.simulate_keystrokes("escape");

    let control = cx.debug_bounds(CONTROL_SELECTOR).expect("control bounds");
    cx.simulate_click(control.center(), Default::default());

    assert_eq!(
        port_events.pointer.borrow().as_slice(),
        [
            OpenGpuiPortDragPhase::Start,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
        ],
    );
    assert_eq!(control_events.borrow().as_slice(), ["down", "up"]);
    assert!(node_events.borrow().is_empty());
    assert!(canvas_events.borrow().is_empty());
}

#[test]
fn port_element_identity_is_stable_per_instance_and_changes_after_recreation() {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let first_frame = view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .unwrap();
    let first_instance = first_frame.nodes()[0].instance().clone();
    let first_host = EntityId::from(1);
    let first_id = open_gpui_port_element_id(first_host, &first_instance, output_id());

    assert_eq!(
        first_id,
        open_gpui_port_element_id(first_host, &first_instance, output_id())
    );
    assert_ne!(
        first_id,
        open_gpui_port_element_id(EntityId::from(2), &first_instance, output_id())
    );

    let snapshot = runtime.snapshot().unwrap();
    let node = snapshot.graph().node(&node_id()).unwrap().clone();
    let port = snapshot.graph().port(&output_id()).unwrap().clone();
    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    runtime
        .execute(GraphCommand::topology(
            1,
            TopologyEdit::new().add_node(node_id(), node, [TopologyPort::new(output_id(), port)]),
        ))
        .unwrap();

    let second_frame = view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .unwrap();
    let second_instance = second_frame.nodes()[0].instance();
    assert_ne!(first_instance.incarnation(), second_instance.incarnation());
    assert_ne!(
        first_id,
        open_gpui_port_element_id(first_host, second_instance, output_id())
    );
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
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let port_events = Rc::new(PortProbe::enabled());
    let node_events = Rc::new(RefCell::new(Vec::new()));
    let canvas_events = Rc::new(RefCell::new(Vec::new()));
    let control_events = Rc::new(RefCell::new(Vec::new()));
    let scroll_events = Rc::new(RefCell::new(Vec::new()));
    let control_focus = Rc::new(RefCell::new(None));
    let factory_port_events = port_events.clone();
    let factory_node_events = node_events.clone();
    let factory_control_events = control_events.clone();
    let factory_scroll_events = scroll_events.clone();
    let factory_control_focus = control_focus.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let control_focus = cx.focus_handle().tab_stop(true);
        *factory_control_focus.borrow_mut() = Some(control_focus.downgrade());
        cx.new(|_| InteractiveNode {
            binding,
            control_focus,
            port: factory_port_events.clone(),
            node_events: factory_node_events.clone(),
            control_events: factory_control_events.clone(),
            scroll_events: factory_scroll_events.clone(),
        })
    });

    let captured_host = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let scene_canvas_events = canvas_events.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
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
        keystroke: Keystroke::parse(key).expect("valid test keystroke"),
    });
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
        .register(
            NodeKindSpec::builder(KIND, "Interactive node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
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
