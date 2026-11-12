use std::cell::RefCell;

use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, HitboxId, InspectorElementId, IntoElement, LayoutId, Pixels,
    PointerCaptureHandle,
};

use crate::activity::{OpenGpuiNodeActivityHandle, OpenGpuiPointerCaptureLease};
use crate::presentation::OpenGpuiNodeLod;

use super::*;

const POINTER_CAPTURE_SELECTOR: &str = "jellyflow-test-pointer-capture-node";

struct PointerCaptureNode {
    activity: OpenGpuiNodeActivityHandle,
    handle: PointerCaptureHandle,
    capture: Rc<RefCell<Option<OpenGpuiPointerCaptureLease>>>,
    hitboxes: Rc<RefCell<Vec<HitboxId>>>,
    releases: Rc<Cell<u32>>,
}

impl Render for PointerCaptureNode {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        PointerCaptureElement {
            activity: self.activity.clone(),
            handle: self.handle,
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
    handle: PointerCaptureHandle,
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
        window
            .bind_pointer_capture(&self.handle, hitbox.id)
            .expect("bind retained pointer control to its current-frame hitbox");
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
        let pointer_hitbox = hitbox.clone();
        let activity = self.activity.clone();
        let handle = self.handle;
        let capture = self.capture.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !pointer_hitbox.is_mouse_event_target(window)
                || window.default_prevented()
                || capture.borrow().is_some()
            {
                return;
            }

            let lease = activity
                .capture_pointer(&handle, event.button, window)
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
                || !pointer_hitbox.is_mouse_event_target(window)
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
    PointerCaptureHandle,
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
    register_test_renderer(&mut registry, move |binding, window, cx| {
        *factory_binding.borrow_mut() = Some(binding.clone());
        let activity = binding.read(cx).activity();
        let handle = window.new_pointer_capture_handle();
        cx.new(|_| PointerCaptureNode {
            activity,
            handle,
            capture: factory_capture.clone(),
            hitboxes: factory_hitboxes.clone(),
            releases: factory_releases.clone(),
        })
    });
    let captured_host = Rc::new(RefCell::new(None));
    let scene_host = captured_host.clone();
    let (scene, cx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal(&catalog())
                    .expect("valid pointer-capture registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        ReplaceableInteractiveScene {
            host: Some(host),
            pointer_capture: window.new_pointer_capture_handle(),
            detached_capture: None,
            focus: cx.focus_handle(),
        }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    let successor = cx.read_entity(&scene, |scene, _| scene.pointer_capture);
    let _ = cx.update(|window, _| window.activate_window());
    draw(cx);
    draw(cx);
    let binding = binding.borrow().clone().expect("captured node binding");
    (
        runtime, host, binding, capture, hitboxes, releases, successor, cx,
    )
}

#[open_gpui::test]
fn pointer_capture_survives_hitbox_replacement_with_exact_ledger_entry_and_close_releases_it(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, binding, capture, hitboxes, _releases, _successor, cx) =
        mount_pointer_capture_scene(cx);
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let first = *hitboxes.borrow().last().expect("initial prepaint hitbox");
    let bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
    let first_capture = {
        let capture = capture.borrow();
        let lease = capture.as_ref().expect("registered pointer capture");
        (lease.handle(), lease.button())
    };
    assert_eq!(first_capture.1, MouseButton::Left);
    assert_eq!(activity.active_count(), 1);
    assert_eq!(activity.protected_count(), 1);

    cx.update(|window, _| window.refresh());
    draw(cx);
    let replacement = *hitboxes
        .borrow()
        .last()
        .expect("replacement prepaint hitbox");
    assert_ne!(first, replacement);
    {
        let capture = capture.borrow();
        let lease = capture
            .as_ref()
            .expect("capture survives hitbox replacement");
        assert_eq!((lease.handle(), lease.button()), first_capture);
    }
    assert_eq!(
        cx.update(|window, _| {
            window
                .captured_pointer()
                .map(|capture| (capture.handle(), capture.button()))
        }),
        Some(first_capture),
    );
    assert_eq!(activity.active_count(), 1);
    assert_eq!(activity.protected_count(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("window-aware close releases capture after hitbox replacement");
        });
    });

    assert!(cx.update(|window, _| window.captured_pointer()).is_none());
    assert!(activity.is_revoked());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    drop(capture.borrow_mut().take());
}

#[open_gpui::test]
fn pointer_capture_redraw_does_not_reclaim_a_replacement_owner(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, _binding, capture, hitboxes, _releases, successor, cx) =
        mount_pointer_capture_scene(cx);
    let first = *hitboxes.borrow().last().expect("initial prepaint hitbox");
    let bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
    let first_capture = {
        let capture = capture.borrow();
        let lease = capture.as_ref().expect("registered pointer capture");
        (lease.handle(), lease.button())
    };
    assert_eq!(first_capture.1, MouseButton::Left);
    assert_ne!(first_capture.0, successor);
    cx.update(|window, _| window.refresh());
    draw(cx);
    let requested = *hitboxes
        .borrow()
        .last()
        .expect("requested replacement hitbox");
    assert_ne!(first, requested);

    cx.update(|window, _| {
        assert!(
            window
                .release_pointer(&first_capture.0)
                .expect("release original capture before successor"),
        );
        window
            .capture_pointer(&successor, MouseButton::Left)
            .expect("bound replacement owner captures the pressed pointer");
        window.refresh();
    });
    draw(cx);
    let replacement = *hitboxes.borrow().last().expect("replacement owner hitbox");
    assert_ne!(first, replacement);
    assert_ne!(replacement, requested);

    {
        let capture = capture.borrow();
        let lease = capture.as_ref().expect("original lease survives redraw");
        assert_eq!((lease.handle(), lease.button()), first_capture);
    }
    assert_eq!(
        cx.update(|window, _| {
            window
                .captured_pointer()
                .map(|capture| (capture.handle(), capture.button()))
        }),
        Some((successor, MouseButton::Left)),
    );

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("close must not release another owner's capture");
        });
    });
    let lease = capture.borrow_mut().take().expect("original capture lease");
    assert!(!cx.update(|window, _| lease.release(window)));
    assert_eq!(
        cx.update(|window, _| {
            window
                .captured_pointer()
                .map(|capture| (capture.handle(), capture.button()))
        }),
        Some((successor, MouseButton::Left)),
    );
    cx.update(|window, _| {
        assert!(
            window
                .release_pointer(&successor)
                .expect("release successor capture"),
        );
    });
}

#[open_gpui::test]
fn pointer_capture_retains_detail_across_overview_and_receives_outside_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, binding, capture, hitboxes, releases, _successor, cx) =
        mount_pointer_capture_scene(cx);
    let detail_bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");
    let first_hitbox = *hitboxes.borrow().last().expect("detail prepaint hitbox");

    cx.simulate_mouse_down(
        detail_bounds.center(),
        MouseButton::Left,
        Default::default(),
    );
    let first_capture = {
        let capture = capture.borrow();
        let lease = capture
            .as_ref()
            .expect("detail control captured the pointer");
        (lease.handle(), lease.button())
    };
    assert_eq!(first_capture.1, MouseButton::Left);

    cx.read_entity(&host, |host, _| {
        host.view
            .set_viewport(ViewportTransform::new(CanvasPoint::default(), 0.5).unwrap())
            .expect("set overview viewport");
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let replacement_hitbox = *hitboxes
        .borrow()
        .last()
        .expect("pinned detail replacement hitbox");
    assert_ne!(replacement_hitbox, first_hitbox);
    {
        let capture = capture.borrow();
        let lease = capture
            .as_ref()
            .expect("overview transition retained pointer capture");
        assert_eq!((lease.handle(), lease.button()), first_capture);
    }
    assert_eq!(
        cx.update(|window, _| {
            window
                .captured_pointer()
                .map(|capture| (capture.handle(), capture.button()))
        }),
        Some(first_capture),
    );
    assert!(cx.debug_bounds(POINTER_CAPTURE_SELECTOR).is_some());
    cx.read_entity(&binding, |binding, _| {
        assert_eq!(
            binding.presentation().node().lod(),
            OpenGpuiNodeLod::PinnedDetail,
        );
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
    assert!(cx.update(|window, _| window.captured_pointer()).is_none());
    assert!(cx.debug_bounds(POINTER_CAPTURE_SELECTOR).is_none());
    cx.read_entity(&binding, |binding, _| {
        assert_eq!(
            binding.presentation().node().lod(),
            OpenGpuiNodeLod::Overview,
        );
    });
}

#[open_gpui::test]
fn retiring_a_node_releases_its_pointer_capture_after_hitbox_replacement(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, binding, capture, hitboxes, _releases, _successor, cx) =
        mount_pointer_capture_scene(cx);
    let activity = cx.read_entity(&binding, |binding, _| binding.activity());
    let bounds = cx
        .debug_bounds(POINTER_CAPTURE_SELECTOR)
        .expect("detail pointer control bounds");
    let first_hitbox = *hitboxes.borrow().last().expect("initial prepaint hitbox");
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Default::default());
    let first_capture = {
        let capture = capture.borrow();
        let lease = capture.as_ref().expect("active capture before redraw");
        (lease.handle(), lease.button())
    };
    assert_eq!(first_capture.1, MouseButton::Left);
    cx.update(|window, _| window.refresh());
    draw(cx);
    let replacement_hitbox = *hitboxes
        .borrow()
        .last()
        .expect("replacement prepaint hitbox");
    assert_ne!(first_hitbox, replacement_hitbox);
    {
        let capture = capture.borrow();
        let lease = capture.as_ref().expect("active capture after redraw");
        assert_eq!((lease.handle(), lease.button()), first_capture);
    }
    assert_eq!(
        cx.update(|window, _| {
            window
                .captured_pointer()
                .map(|capture| (capture.handle(), capture.button()))
        }),
        Some(first_capture),
    );

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("graph before node retirement")
                .version(),
            TopologyEdit::new().remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("remove captured node")
        .applied()
        .expect("captured node removal applies");
    cx.run_until_parked();
    draw(cx);

    assert!(cx.update(|window, _| window.captured_pointer()).is_none());
    assert!(activity.is_revoked());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    drop(capture.borrow_mut().take());
}
