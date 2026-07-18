use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{GraphRuntime, ViewSessionOptions};
use jellyflow_open_gpui::{
    OpenGpuiNodeBinding, OpenGpuiNodeRegistry, OpenGpuiOverlayInteractionBoundary,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragPhase, OpenGpuiPortEvent, OpenGpuiSceneHost,
};
use open_gpui::{
    AppContext, Context, Entity, MouseButton, ParentElement, Render, Styled, Window, deferred, div,
    point, prelude::*, px,
};

const KIND: &str = "test.overlay-owner";
const RENDERER: &str = "test.native.overlay-owner";
const PORT_SELECTOR: &str = "jellyflow-test-overlay-underlay-port";
const OVERLAY_SELECTOR: &str = "jellyflow-test-owned-overlay";
const OVERLAY_CONTROL_SELECTOR: &str = "jellyflow-test-owned-overlay-control";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservedEvent {
    Port(OpenGpuiPortDragPhase),
    NodeDown,
    NodeUp,
    CanvasDown,
    CanvasUp,
    OverlayDown,
    OverlayUp,
    OverlayControlDown,
    OverlayControlUp,
}

type EventLog = Rc<RefCell<Vec<ObservedEvent>>>;

struct UnderlayNode {
    binding: Entity<OpenGpuiNodeBinding>,
    events: EventLog,
}

struct OverlayScene {
    host: Entity<OpenGpuiSceneHost>,
    overlay_open: Rc<Cell<bool>>,
    events: EventLog,
}

impl Render for UnderlayNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let port = self.binding.read(cx).port_handle(output_id());
        let node_down = self.events.clone();
        let node_up = self.events.clone();
        let port_events = self.events.clone();

        div()
            .relative()
            .size_full()
            .on_any_mouse_down(move |_, _, _| node_down.borrow_mut().push(ObservedEvent::NodeDown))
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                node_up.borrow_mut().push(ObservedEvent::NodeUp)
            })
            .children(port.map(|handle| {
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
                            move |event, _, _| {
                                let OpenGpuiPortEvent::Pointer(event) = event else {
                                    return;
                                };
                                port_events
                                    .borrow_mut()
                                    .push(ObservedEvent::Port(event.phase()));
                            },
                        ),
                    )
                    .into_any_element()
            }))
    }
}

impl Render for OverlayScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let canvas_down = self.events.clone();
        let canvas_up = self.events.clone();
        let overlay = self.overlay_open.get().then(|| {
            let overlay_down = self.events.clone();
            let overlay_up = self.events.clone();
            let control_down = self.events.clone();
            let control_up = self.events.clone();

            deferred(OpenGpuiOverlayInteractionBoundary::wrap(
                div()
                    .debug_selector(|| OVERLAY_SELECTOR.to_owned())
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .w(px(800.0))
                    .h(px(600.0))
                    .on_mouse_down(MouseButton::Left, move |_, _, _| {
                        overlay_down.borrow_mut().push(ObservedEvent::OverlayDown)
                    })
                    .on_mouse_up(MouseButton::Left, move |_, _, _| {
                        overlay_up.borrow_mut().push(ObservedEvent::OverlayUp)
                    })
                    .child(
                        div()
                            .debug_selector(|| OVERLAY_CONTROL_SELECTOR.to_owned())
                            .absolute()
                            .left(px(280.0))
                            .top(px(120.0))
                            .w(px(120.0))
                            .h(px(36.0))
                            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                                control_down
                                    .borrow_mut()
                                    .push(ObservedEvent::OverlayControlDown)
                            })
                            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                                control_up
                                    .borrow_mut()
                                    .push(ObservedEvent::OverlayControlUp)
                            }),
                    ),
            ))
            .with_priority(100)
        });

        div()
            .relative()
            .size_full()
            .on_any_mouse_down(move |_, _, _| {
                canvas_down.borrow_mut().push(ObservedEvent::CanvasDown)
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                canvas_up.borrow_mut().push(ObservedEvent::CanvasUp)
            })
            .child(self.host.clone())
            .children(overlay)
    }
}

#[open_gpui::test]
fn overlay_control_and_blank_space_own_events_before_port_node_and_canvas(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, scene, cx, overlay_open, events) = mount_scene(cx);
    draw(cx);
    let port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("underlay port bounds");

    open_overlay(&scene, &overlay_open, cx);
    let overlay = cx.debug_bounds(OVERLAY_SELECTOR).expect("overlay bounds");
    let control = cx
        .debug_bounds(OVERLAY_CONTROL_SELECTOR)
        .expect("overlay control bounds");
    assert!(overlay.contains(&port.center()));
    assert!(overlay.contains(&control.center()));

    cx.simulate_click(control.center(), Default::default());
    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::OverlayControlDown,
            ObservedEvent::OverlayDown,
            ObservedEvent::OverlayControlUp,
            ObservedEvent::OverlayUp,
        ]
    );

    events.borrow_mut().clear();
    cx.simulate_click(port.center(), Default::default());
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );

    events.borrow_mut().clear();
    cx.simulate_mouse_down(
        point(px(500.0), px(400.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(560.0), px(460.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(
        point(px(560.0), px(460.0)),
        MouseButton::Left,
        Default::default(),
    );
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_none()));
}

#[open_gpui::test]
fn active_port_capture_precedes_a_new_overlay_until_explicit_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, scene, cx, overlay_open, events) = mount_scene(cx);
    draw(cx);
    let port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("underlay port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    let owner = cx.read_entity(&host, |host, _| {
        host.active_port_drag().expect("port capture owner")
    });
    assert_eq!(owner.instance().node(), node_id());
    assert_eq!(owner.port(), output_id());

    open_overlay(&scene, &overlay_open, cx);
    let control = cx
        .debug_bounds(OVERLAY_CONTROL_SELECTOR)
        .expect("overlay control bounds");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.active_port_drag(), Some(owner.clone()))
    });
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    cx.simulate_mouse_move(
        control.center(),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(control.center(), MouseButton::Left, Default::default());

    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::Port(OpenGpuiPortDragPhase::Start),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Move),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Release),
        ]
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_none()));

    events.borrow_mut().clear();
    cx.simulate_click(control.center(), Default::default());
    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::OverlayControlDown,
            ObservedEvent::OverlayDown,
            ObservedEvent::OverlayControlUp,
            ObservedEvent::OverlayUp,
        ]
    );
}

#[open_gpui::test]
fn escape_cancels_capture_before_the_overlay_becomes_the_pointer_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, scene, cx, overlay_open, events) = mount_scene(cx);
    draw(cx);
    let port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("underlay port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    open_overlay(&scene, &overlay_open, cx);
    cx.simulate_keystrokes("escape");

    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::Port(OpenGpuiPortDragPhase::Start),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Cancel(
                OpenGpuiPortDragCancelReason::Escape,
            )),
        ]
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_none()));

    events.borrow_mut().clear();
    cx.simulate_click(port.center(), Default::default());
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );
}

fn mount_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    Entity<OverlayScene>,
    &mut open_gpui::VisualTestContext,
    Rc<Cell<bool>>,
    EventLog,
) {
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph())
            .expect("valid overlay ownership graph"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach overlay ownership view");
    let events = Rc::new(RefCell::new(Vec::new()));
    let factory_events = events.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| UnderlayNode {
            binding,
            events: factory_events.clone(),
        })
    });

    let overlay_open = Rc::new(Cell::new(false));
    let scene_overlay_open = overlay_open.clone();
    let scene_events = events.clone();
    let captured_host = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        OverlayScene {
            host,
            overlay_open: scene_overlay_open,
            events: scene_events,
        }
    });
    let host = captured_host.borrow().clone().expect("captured scene host");
    (runtime, host, scene, cx, overlay_open, events)
}

fn open_overlay(
    scene: &Entity<OverlayScene>,
    overlay_open: &Cell<bool>,
    cx: &mut open_gpui::VisualTestContext,
) {
    overlay_open.set(true);
    cx.update_entity(scene, |_, cx| cx.notify());
    draw(cx);
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
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
        data: serde_json::json!({ "label": "overlay underlay" }),
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
    graph.build().expect("valid overlay ownership graph")
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
            NodeKindSpec::builder(KIND, "Overlay underlay", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
        .expect("register overlay ownership node kind");
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
