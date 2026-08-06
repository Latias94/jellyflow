use std::cell::RefCell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_open_gpui::activity::{OpenGpuiActivityKind, OpenGpuiNodeActivityHandle};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    graph::GraphRuntime, interaction::CanvasInteractionPhase, view::RendererStamp,
    view::ViewSessionOptions, view::WeakViewSession,
};
use open_gpui::{
    AppContext, Context, Entity, FocusHandle, MouseButton, ParentElement, Render, Styled, Window,
    div, point, px,
};

const KIND: &str = "test.native-activity-keyboard-owner";
const RENDERER: &str = "test.native.activity-keyboard-owner";

struct ActivityNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for ActivityNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full()
    }
}

struct ActivityScene {
    host: Entity<OpenGpuiSceneHost>,
}

impl Render for ActivityScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child(self.host.clone())
    }
}

#[open_gpui::test]
fn ime_lease_owns_graph_keys_until_release(cx: &mut open_gpui::TestAppContext) {
    assert_activity_lease_owns_graph_keys(cx, OpenGpuiActivityKind::Ime);
}

#[open_gpui::test]
fn overlay_lease_owns_graph_keys_until_release(cx: &mut open_gpui::TestAppContext) {
    assert_activity_lease_owns_graph_keys(cx, OpenGpuiActivityKind::Overlay);
}

fn assert_activity_lease_owns_graph_keys(
    cx: &mut open_gpui::TestAppContext,
    kind: OpenGpuiActivityKind,
) {
    let (runtime, host, view, activity, root_focus, cx) = mount_scene(cx);
    draw(cx);
    draw(cx);
    let activity = activity
        .borrow()
        .clone()
        .expect("mounted node activity handle");
    let mut lease = Some(activity.acquire(kind).expect("activity lease"));
    select_node(&view);
    cx.update(|window, cx| window.focus(&root_focus, cx));

    let before = runtime.snapshot().expect("graph before leased input");
    let position_before = before
        .graph()
        .node(&node_id())
        .expect("node before leased input")
        .pos;
    let history_before = runtime
        .history_state()
        .expect("history before leased input");

    cx.simulate_keystrokes("right");
    cx.simulate_keystrokes("delete");
    settle_graph_commands(cx);

    assert!(
        before.ptr_eq(
            &runtime
                .snapshot()
                .expect("graph after lease-owned direction and delete keys"),
        ),
        "{kind:?} must prevent graph mutation while its lease is active",
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after lease-owned direction and delete keys"),
        history_before,
    );

    let blank = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(blank, MouseButton::Left, Default::default());
    if kind == OpenGpuiActivityKind::Overlay {
        assert_eq!(
            cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
            CanvasInteractionPhase::Idle,
            "an overlay lease must block new graph-pointer ownership beneath the overlay",
        );
        drop(lease.take());
        draw(cx);
        cx.simulate_mouse_down(blank, MouseButton::Left, Default::default());
        lease = Some(activity.acquire(kind).expect("replacement overlay lease"));
        draw(cx);
    }
    let active_phase = cx.read_entity(&host, |host, _| host.interaction_presentation().phase());
    assert_ne!(active_phase, CanvasInteractionPhase::Idle);

    cx.simulate_keystrokes("escape");
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        active_phase,
        "{kind:?} must prevent Escape from cancelling the graph interaction",
    );

    drop(lease.take());
    draw(cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        CanvasInteractionPhase::Idle,
        "graph Escape must recover immediately after the {kind:?} lease drops",
    );
    cx.simulate_mouse_up(blank, MouseButton::Left, Default::default());

    select_node(&view);
    cx.update(|window, cx| window.focus(&root_focus, cx));
    cx.simulate_keystrokes("right");
    settle_graph_commands(cx);
    let nudged = runtime.snapshot().expect("graph after released nudge");
    let position_after = nudged
        .graph()
        .node(&node_id())
        .expect("node after released nudge")
        .pos;
    assert_eq!(position_after.x, position_before.x + 10.0);
    assert_eq!(position_after.y, position_before.y);

    cx.simulate_keystrokes("delete");
    settle_graph_commands(cx);
    assert!(
        runtime
            .snapshot()
            .expect("graph after released delete")
            .graph()
            .node(&node_id())
            .is_none(),
        "graph Delete must recover immediately after the {kind:?} lease drops",
    );
}

fn mount_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    WeakViewSession,
    Rc<RefCell<Option<OpenGpuiNodeActivityHandle>>>,
    FocusHandle,
    &mut open_gpui::VisualTestContext,
) {
    let node_catalog = catalog();
    let runtime = Rc::new(
        GraphRuntime::builder(node_catalog.clone())
            .build(graph())
            .expect("valid activity ownership graph"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach activity ownership view");
    let view_handle = view.downgrade();
    let activity = Rc::new(RefCell::new(None));
    let factory_activity = activity.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Activity owner"),
            move |binding, _window, cx| {
                *factory_activity.borrow_mut() = Some(binding.read(cx).activity());
                Ok(cx.new(|_| ActivityNode { _binding: binding }))
            },
        ))
        .expect("unique activity ownership renderer");

    let captured_host = Rc::new(RefCell::new(None));
    let captured_focus = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let scene_focus = captured_focus.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let root_focus = cx.focus_handle().tab_stop(true);
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&node_catalog).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
            .with_graph_focus_handle(root_focus.clone())
        });
        *scene_host.borrow_mut() = Some(host.clone());
        *scene_focus.borrow_mut() = Some(root_focus);
        ActivityScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured scene host");
    let root_focus = captured_focus
        .borrow()
        .clone()
        .expect("captured graph focus");
    (runtime, host, view_handle, activity, root_focus, cx)
}

fn select_node(view: &WeakViewSession) {
    let snapshot = view.graph_snapshot().expect("current graph for selection");
    let lifetime = snapshot
        .node_lifetime_key(node_id())
        .expect("current activity node lifetime");
    view.set_selection(vec![lifetime], Vec::new(), Vec::new())
        .expect("select activity node");
}

fn settle_graph_commands(cx: &mut open_gpui::VisualTestContext) {
    for _ in 0..4 {
        draw(cx);
    }
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
        ports: Vec::new(),
        data: serde_json::json!({ "label": "activity owner" }),
    };
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node_id(), node);
    graph.build().expect("valid activity ownership graph")
}

fn node_id() -> NodeId {
    NodeId::from_u128(10)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .expect("register activity ownership node kind");
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
