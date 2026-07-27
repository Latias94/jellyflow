use std::cell::RefCell;

use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels,
};

use crate::activity::{OpenGpuiNodeActivityHandle, OpenGpuiPointerCaptureLease};
use crate::presentation::OpenGpuiNodeLod;
use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::*;

const POINTER_CAPTURE_SELECTOR: &str = "jellyflow-test-pointer-capture-node";

struct PointerCaptureNode {
    activity: OpenGpuiNodeActivityHandle,
    capture: Rc<RefCell<Option<OpenGpuiPointerCaptureLease>>>,
    hitboxes: Rc<RefCell<Vec<HitboxId>>>,
    releases: Rc<Cell<u32>>,
}

impl Render for PointerCaptureNode {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        PointerCaptureElement {
            activity: self.activity.clone(),
            capture: self.capture.clone(),
            hitboxes: self.hitboxes.clone(),
            releases: self.releases.clone(),
            child: Some(
                div()
                    .debug_selector(|| POINTER_CAPTURE_SELECTOR.to_owned())
                    .size_full()
                    .into_any_element(),
            ),
        }
    }
}

struct PointerCaptureElement {
    activity: OpenGpuiNodeActivityHandle,
    capture: Rc<RefCell<Option<OpenGpuiPointerCaptureLease>>>,
    hitboxes: Rc<RefCell<Vec<HitboxId>>>,
    releases: Rc<Cell<u32>>,
    child: Option<AnyElement>,
}

impl Element for PointerCaptureElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = self.child.take().expect("pointer-capture child missing");
        let layout = child.request_layout(window, cx);
        (layout, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.hitboxes.borrow_mut().push(hitbox.id);
        child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(capture) = self.capture.borrow_mut().as_mut() {
            assert!(
                capture.rebind(hitbox.id, window),
                "a retained detail control must transfer its still-owned capture after redraw",
            );
        }

        let pointer_hitbox = hitbox.clone();
        let activity = self.activity.clone();
        let capture = self.capture.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !pointer_hitbox.is_hovered(window)
                || window.default_prevented()
                || capture.borrow().is_some()
            {
                return;
            }

            let lease = activity
                .capture_pointer(pointer_hitbox.id, window)
                .expect("test node pointer capture");
            *capture.borrow_mut() = Some(lease);
            window.prevent_default();
            window.refresh();
            cx.stop_propagation();
        });

        let pointer_hitbox = hitbox.clone();
        let capture = self.capture.clone();
        let releases = self.releases.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !pointer_hitbox.is_hovered(window)
            {
                return;
            }
            let Some(lease) = capture.borrow_mut().take() else {
                return;
            };

            assert!(lease.release(window));
            releases.set(releases.get().saturating_add(1));
            window.refresh();
            cx.stop_propagation();
        });

        child.paint(window, cx);
    }
}

impl IntoElement for PointerCaptureElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn mount_pointer_capture_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    Entity<OpenGpuiNodeBinding>,
    Rc<RefCell<Option<OpenGpuiPointerCaptureLease>>>,
    Rc<RefCell<Vec<HitboxId>>>,
    Rc<Cell<u32>>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(single_node_graph())
        .expect("valid pointer-capture graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach pointer-capture view");
    let binding = Rc::new(RefCell::new(None));
    let factory_binding = binding.clone();
    let capture = Rc::new(RefCell::new(None));
    let factory_capture = capture.clone();
    let hitboxes = Rc::new(RefCell::new(Vec::new()));
    let factory_hitboxes = hitboxes.clone();
    let releases = Rc::new(Cell::new(0));
    let factory_releases = releases.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        *factory_binding.borrow_mut() = Some(binding.clone());
        let activity = binding.read(cx).activity();
        cx.new(|_| PointerCaptureNode {
            activity,
            capture: factory_capture.clone(),
            hitboxes: factory_hitboxes.clone(),
            releases: factory_releases.clone(),
        })
    });
    let captured_host = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid pointer-capture registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    draw(cx);
    let binding = binding.borrow().clone().expect("captured node binding");
    (runtime, host, binding, capture, hitboxes, releases, cx)
}

#[open_gpui::test]
fn pointer_capture_rebind_transfers_the_exact_ledger_entry_and_close_releases_it(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, binding, _capture, hitboxes, _releases, cx) =
        mount_pointer_capture_scene(cx);
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let first = *hitboxes.borrow().last().expect("initial prepaint hitbox");

    let mut lease = cx.update(|window, _| {
        activity
            .capture_pointer(first, window)
            .expect("registered pointer capture")
    });
    draw(cx);
    let rebound = *hitboxes
        .borrow()
        .last()
        .expect("replacement prepaint hitbox");
    assert_ne!(first, rebound);
    assert!(cx.update(|window, _| lease.rebind(rebound, window)));
    assert_eq!(lease.hitbox(), rebound);
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(rebound),
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("window-aware close releases rebound capture");
        });
    });

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert!(activity.is_revoked());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });
    drop(lease);
}

#[open_gpui::test]
fn pointer_capture_rebind_does_not_reclaim_a_replacement_owner(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, binding, _capture, hitboxes, _releases, cx) =
        mount_pointer_capture_scene(cx);
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let first = *hitboxes.borrow().last().expect("initial prepaint hitbox");
    let mut lease = cx.update(|window, _| {
        activity
            .capture_pointer(first, window)
            .expect("registered pointer capture")
    });
    draw(cx);
    let requested = *hitboxes
        .borrow()
        .last()
        .expect("requested replacement hitbox");
    draw(cx);
    let replacement = *hitboxes.borrow().last().expect("replacement owner hitbox");
    assert_ne!(first, replacement);
    assert_ne!(replacement, requested);

    cx.update(|window, _| window.capture_pointer(replacement));

    assert!(!cx.update(|window, _| lease.rebind(requested, window)));
    assert_eq!(lease.hitbox(), first);
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(replacement),
    );

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("close must not release another owner's capture");
        });
    });
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(replacement),
    );
    cx.update(|window, _| window.release_pointer());
    drop(lease);
}

#[open_gpui::test]
fn pointer_capture_retains_detail_across_overview_and_receives_outside_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, binding, capture, _hitboxes, releases, cx) =
        mount_pointer_capture_scene(cx);
    let detail_bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");

    cx.simulate_mouse_down(
        detail_bounds.center(),
        MouseButton::Left,
        Default::default(),
    );
    let first_capture = capture
        .borrow()
        .as_ref()
        .expect("detail control captured the pointer")
        .hitbox();

    cx.read_entity(&host, |host, _| {
        host.view
            .set_viewport(ViewportTransform::new(CanvasPoint::default(), 0.5).unwrap())
            .expect("set overview viewport");
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let rebound_capture = capture
        .borrow()
        .as_ref()
        .expect("overview transition retained pointer capture")
        .hitbox();
    assert_ne!(rebound_capture, first_capture);
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(rebound_capture),
    );
    assert!(cx.debug_bounds(POINTER_CAPTURE_SELECTOR).is_some());
    cx.read_entity(&binding, |binding, _| {
        assert_eq!(
            binding.presentation().node().lod(),
            OpenGpuiNodeLod::InteractionIsland,
        );
    });
    cx.read_entity(&host, |host, _| {
        let mounted = host
            .mounts
            .active_in_order()
            .find(|(instance, _)| instance.node() == NodeId::from_u128(10))
            .map(|(_, mounted)| mounted)
            .expect("mounted overview node");
        assert_eq!(
            mounted.graph_presentation(),
            OpenGpuiNodePresentation::Overview,
        );
        assert!(mounted.has_interaction_island());
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    assert_eq!(releases.get(), 1);
    assert!(capture.borrow().is_none());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert!(cx.debug_bounds(POINTER_CAPTURE_SELECTOR).is_none());
    cx.read_entity(&binding, |binding, _| {
        assert_eq!(
            binding.presentation().node().lod(),
            OpenGpuiNodeLod::Overview,
        );
    });
    cx.read_entity(&host, |host, _| {
        let mounted = host
            .mounts
            .active_in_order()
            .find(|(instance, _)| instance.node() == NodeId::from_u128(10))
            .map(|(_, mounted)| mounted)
            .expect("mounted overview node after release");
        assert_eq!(
            mounted.graph_presentation(),
            OpenGpuiNodePresentation::Overview,
        );
        assert!(!mounted.has_interaction_island());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn retiring_a_node_releases_its_rebound_pointer_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, host, binding, capture, _hitboxes, _releases, cx) =
        mount_pointer_capture_scene(cx);
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
    let first_capture = capture
        .borrow()
        .as_ref()
        .expect("active capture before redraw")
        .hitbox();
    draw(cx);
    let rebound_capture = capture
        .borrow()
        .as_ref()
        .expect("active capture after redraw")
        .hitbox();
    assert_ne!(first_capture, rebound_capture);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("graph before node retirement")
                .version(),
            TopologyEdit::new().remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject),
        ))
        .expect("remove captured node")
        .applied()
        .expect("captured node removal applies");
    cx.run_until_parked();
    draw(cx);

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    assert!(activity.is_revoked());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().hard_retirements(), 1);
    });
    drop(capture.borrow_mut().take());
}
