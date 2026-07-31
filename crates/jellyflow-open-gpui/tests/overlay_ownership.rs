use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::{OpenGpuiConnectionOptions, OpenGpuiSceneHost};
use jellyflow_open_gpui::interaction::{OpenGpuiPortDragPhase, OpenGpuiPortEvent};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    graph::GraphRuntime, interaction::CanvasInteractionPhase, view::RendererStamp,
    view::ViewSessionOptions, view::WeakViewSession, viewport::ViewportTransform,
};
use open_gpui::{
    App, AppContext, Context, Entity, MouseButton, ParentElement, Render, Styled, Window, deferred,
    div, point, prelude::*, px,
};

const KIND: &str = "test.overlay-owner";
const RENDERER: &str = "test.native.overlay-owner";
const NODE_SELECTOR: &str = "jellyflow-test-overlay-underlay-node";
const PORT_SELECTOR: &str = "jellyflow-test-overlay-underlay-port";
const OVERLAY_SELECTOR: &str = "jellyflow-test-owned-overlay";
const OVERLAY_CONTROL_SELECTOR: &str = "jellyflow-test-owned-overlay-control";

fn renderer_descriptor<F, V>(factory: F) -> OpenGpuiNodeRendererDescriptor
where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    OpenGpuiNodeRendererDescriptor::new(
        KIND,
        1,
        RendererStamp::new(RENDERER, 1),
        OpenGpuiNodeAdapterPresentation::new("Overlay underlay"),
        move |binding, window, cx| Ok(factory(binding, window, cx)),
    )
}

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
        let binding = self.binding.read(cx);
        let port = binding.authoring().ok().and_then(|authoring| {
            authoring
                .port(output_id())
                .map(|port| port.handle().clone())
        });
        let drag_handle = binding.drag_handle();
        let node_down = self.events.clone();
        let node_up = self.events.clone();
        let port_events = self.events.clone();

        drag_handle.wrap(
            div()
                .debug_selector(|| NODE_SELECTOR.to_owned())
                .relative()
                .size_full()
                .on_any_mouse_down(move |_, _, _| {
                    node_down.borrow_mut().push(ObservedEvent::NodeDown)
                })
                .on_mouse_up(MouseButton::Left, move |_, _, _| {
                    node_up.borrow_mut().push(ObservedEvent::NodeUp)
                })
                .children(port.map(|handle| {
                    div()
                        .absolute()
                        .right(px(8.0))
                        .top(px(43.0))
                        .child(
                            handle.interactive_with_observer(
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
                })),
        )
    }
}

impl Render for OverlayScene {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let canvas_down = self.events.clone();
        let canvas_up = self.events.clone();
        let overlay = self.overlay_open.get().then(|| {
            let overlay_down = self.events.clone();
            let overlay_up = self.events.clone();
            let control_down = self.events.clone();
            let control_up = self.events.clone();

            deferred(
                self.host.read(cx).overlay_boundary().wrap(
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
                ),
            )
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
    let (runtime, host, scene, cx, overlay_open, events, view) = mount_scene(cx);
    draw(cx);
    let node = cx
        .debug_bounds(NODE_SELECTOR)
        .expect("underlay node drag bounds");
    let port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("underlay port bounds");
    let graph_before = runtime
        .snapshot()
        .expect("graph before overlay interactions");
    let position_before = graph_before
        .graph()
        .node(&node_id())
        .expect("underlay node before overlay interactions")
        .pos;
    let history_before = runtime
        .history_state()
        .expect("history before overlay interactions");
    let selection_before = view
        .selection()
        .expect("selection before overlay interactions");

    let assert_underlay_unchanged = |cx: &mut open_gpui::VisualTestContext| {
        assert!(
            cx.update(|window, _| window.captured_hitbox()).is_none(),
            "an overlay-owned pointer phase must not leave native capture behind",
        );
        cx.read_entity(&host, |host, _| {
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Idle,
                "an overlay-owned pointer phase must not arm the graph host",
            );
            assert!(host.active_port_drag().is_none());
            assert_eq!(host.resource_snapshot().protected_instances(), 0);
        });
        assert!(
            graph_before.ptr_eq(
                &runtime
                    .snapshot()
                    .expect("graph after overlay-owned pointer phase"),
            ),
            "overlay-owned input must not replace the graph root",
        );
        assert_eq!(
            runtime
                .snapshot()
                .expect("graph position after overlay-owned pointer phase")
                .graph()
                .node(&node_id())
                .expect("underlay node after overlay-owned pointer phase")
                .pos,
            position_before,
        );
        assert_eq!(
            runtime
                .history_state()
                .expect("history after overlay-owned pointer phase"),
            history_before,
        );
        assert_eq!(
            view.selection()
                .expect("selection after overlay-owned pointer phase"),
            selection_before,
        );
    };

    open_overlay(&scene, &overlay_open, cx);
    let overlay = cx.debug_bounds(OVERLAY_SELECTOR).expect("overlay bounds");
    let control = cx
        .debug_bounds(OVERLAY_CONTROL_SELECTOR)
        .expect("overlay control bounds");
    assert!(overlay.contains(&port.center()));
    assert!(overlay.contains(&node.center()));
    assert!(overlay.contains(&control.center()));

    cx.simulate_mouse_down(control.center(), MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_up(control.center(), MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
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
    let port_drag_end = point(port.center().x - px(48.0), port.center().y + px(32.0));
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_move(port_drag_end, Some(MouseButton::Left), Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_up(port_drag_end, MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );

    events.borrow_mut().clear();
    let node_drag_end = point(node.center().x + px(52.0), node.center().y + px(36.0));
    cx.simulate_mouse_down(node.center(), MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_move(node_drag_end, Some(MouseButton::Left), Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_up(node_drag_end, MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );

    events.borrow_mut().clear();
    let blank_start = point(px(500.0), px(400.0));
    let blank_end = point(px(560.0), px(460.0));
    assert!(overlay.contains(&blank_start));
    cx.simulate_mouse_down(blank_start, MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_move(blank_end, Some(MouseButton::Left), Default::default());
    assert_underlay_unchanged(cx);
    cx.simulate_mouse_up(blank_end, MouseButton::Left, Default::default());
    assert_underlay_unchanged(cx);
    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::OverlayDown, ObservedEvent::OverlayUp]
    );
    close_overlay(&scene, &overlay_open, cx);
    events.borrow_mut().clear();
    let live_node = cx
        .debug_bounds(NODE_SELECTOR)
        .expect("live node drag bounds after closing overlay");
    let node_drag_start = live_node.center();
    let node_drag_end = point(node_drag_start.x + px(52.0), node_drag_start.y + px(36.0));
    let position_before_node_drag = runtime
        .snapshot()
        .expect("graph before live node drag")
        .graph()
        .node(&node_id())
        .expect("node before live node drag")
        .pos;
    let history_before_node_drag = runtime
        .history_state()
        .expect("history before live node drag");

    cx.simulate_mouse_down(node_drag_start, MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed,
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });
    cx.simulate_mouse_move(node_drag_end, Some(MouseButton::Left), Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });
    cx.simulate_mouse_up(node_drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    let position_after_node_drag = runtime
        .snapshot()
        .expect("graph after live node drag")
        .graph()
        .node(&node_id())
        .expect("node after live node drag")
        .pos;
    assert_ne!(position_after_node_drag, position_before_node_drag);
    assert_eq!(
        runtime
            .history_state()
            .expect("history after live node drag")
            .undo_len(),
        history_before_node_drag.undo_len() + 1,
        "one live node drag must create exactly one history boundary",
    );
    assert_eq!(
        view.selection()
            .expect("selection after live node drag")
            .nodes(),
        [node_id()],
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });

    events.borrow_mut().clear();
    let live_port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("live port bounds after node drag");
    let port_drag_start = live_port.center();
    let port_drag_end = point(px(500.0), px(400.0));
    let graph_before_port_drag = runtime.snapshot().expect("graph before live port drag");
    let history_before_port_drag = runtime
        .history_state()
        .expect("history before live port drag");
    let selection_before_port_drag = view.selection().expect("selection before live port drag");

    cx.simulate_mouse_down(port_drag_start, MouseButton::Left, Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    let port_owner = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed,
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        host.active_port_drag().expect("one live port owner")
    });
    assert_eq!(port_owner.instance().node(), node_id());
    assert_eq!(port_owner.port(), output_id());
    cx.simulate_mouse_move(port_drag_end, Some(MouseButton::Left), Default::default());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
        assert_eq!(host.active_port_drag(), Some(port_owner.clone()));
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
    });
    cx.simulate_mouse_up(port_drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::Port(OpenGpuiPortDragPhase::Start),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Move),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Release),
        ],
        "one live port drag must emit one ordered pointer lifecycle",
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
    assert!(
        graph_before_port_drag.ptr_eq(
            &runtime
                .snapshot()
                .expect("graph after live port drag without a target"),
        ),
        "an unmatched live port drag must not mutate the graph",
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after live port drag"),
        history_before_port_drag,
    );
    assert_eq!(
        view.selection().expect("selection after live port drag"),
        selection_before_port_drag,
    );
}

#[open_gpui::test]
fn open_overlay_blocks_graph_shortcuts_until_its_owner_leaves_the_frame(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, _host, scene, cx, overlay_open, _events, view) = mount_scene(cx);
    draw(cx);
    let node = cx
        .debug_bounds(NODE_SELECTOR)
        .expect("underlay node drag bounds");
    cx.simulate_click(node.center(), Default::default());

    view.set_viewport(
        ViewportTransform::new(
            CanvasPoint {
                x: 240.0,
                y: -160.0,
            },
            2.0,
        )
        .expect("valid non-fit viewport"),
    )
    .expect("set a non-fit viewport");
    draw(cx);
    let before = view
        .viewport_transform()
        .expect("viewport before overlay shortcut");

    open_overlay(&scene, &overlay_open, cx);
    let focused_before_tab = cx.update(|window, cx| window.focused(cx));
    cx.simulate_keystrokes("tab");
    assert_eq!(
        cx.update(|window, cx| window.focused(cx)),
        focused_before_tab,
        "the graph root must not traverse focus behind an open overlay",
    );
    cx.simulate_keystrokes("f");
    assert_eq!(
        view.viewport_transform()
            .expect("viewport while overlay owns input"),
        before,
        "the graph fit shortcut must not leak through an open overlay",
    );

    close_overlay(&scene, &overlay_open, cx);
    cx.simulate_keystrokes("f");
    assert_ne!(
        view.viewport_transform()
            .expect("viewport after overlay closes"),
        before,
        "the graph shortcut becomes live again after the boundary leaves the frame",
    );
}

#[open_gpui::test]
fn active_port_capture_precedes_a_new_overlay_until_explicit_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, scene, cx, overlay_open, events, _view) = mount_scene(cx);
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
fn overlay_owns_escape_while_existing_pointer_capture_waits_for_physical_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, scene, cx, overlay_open, events, _view) = mount_scene(cx);
    draw(cx);
    let port = cx
        .debug_bounds(PORT_SELECTOR)
        .expect("underlay port bounds");
    cx.simulate_mouse_down(port.center(), MouseButton::Left, Default::default());

    open_overlay(&scene, &overlay_open, cx);
    cx.simulate_keystrokes("escape");

    assert_eq!(
        events.borrow().as_slice(),
        [ObservedEvent::Port(OpenGpuiPortDragPhase::Start)]
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| assert!(host.active_port_drag().is_some()));

    let overlay = cx.debug_bounds(OVERLAY_SELECTOR).expect("overlay bounds");
    cx.simulate_mouse_up(overlay.center(), MouseButton::Left, Default::default());
    assert_eq!(
        events.borrow().as_slice(),
        [
            ObservedEvent::Port(OpenGpuiPortDragPhase::Start),
            ObservedEvent::Port(OpenGpuiPortDragPhase::Release),
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
    WeakViewSession,
) {
    let node_catalog = catalog();
    let runtime = Rc::new(
        GraphRuntime::builder(node_catalog.clone())
            .build(graph())
            .expect("valid overlay ownership graph"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach overlay ownership view");
    let view_handle = view.downgrade();
    let events = Rc::new(RefCell::new(Vec::new()));
    let factory_events = events.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
            cx.new(|_| UnderlayNode {
                binding,
                events: factory_events.clone(),
            })
        }))
        .expect("unique overlay-underlay renderer");

    let overlay_open = Rc::new(Cell::new(false));
    let scene_overlay_open = overlay_open.clone();
    let scene_events = events.clone();
    let captured_host = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&node_catalog).expect("valid test registry"),
                viewport(),
            )
            .with_connection_options(OpenGpuiConnectionOptions {
                allow_create_from_wire: false,
                ..OpenGpuiConnectionOptions::default()
            })
        });
        *scene_host.borrow_mut() = Some(host.clone());
        OverlayScene {
            host,
            overlay_open: scene_overlay_open,
            events: scene_events,
        }
    });
    let host = captured_host.borrow().clone().expect("captured scene host");
    (runtime, host, scene, cx, overlay_open, events, view_handle)
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

fn close_overlay(
    scene: &Entity<OverlayScene>,
    overlay_open: &Cell<bool>,
    cx: &mut open_gpui::VisualTestContext,
) {
    overlay_open.set(false);
    cx.update_entity(scene, |_, cx| cx.notify());
    draw(cx);
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
        .register(NodeKindSpec::builder(KIND).build())
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
