use std::cell::Cell;
use std::rc::Rc;

use jellyflow::runtime::NodeInstanceKey;
use open_gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, GlobalElementId,
    Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    Window,
};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::OpenGpuiMountCapability;
use crate::interaction::{
    OpenGpuiNativeInteractionHandle, OpenGpuiOverlayInteractionBoundary, OpenGpuiSceneGestureKind,
    OpenGpuiSceneGestureListener,
};

/// Explicit native drag surface for one retained graph-node instance.
///
/// Ordinary node content is control-safe and never arms movement. Node authors opt into movement
/// by wrapping a header, grip, or deliberately the whole node with [`Self::wrap`]. The handle owns
/// no graph mutation logic; it starts the host's renderer-neutral node-drag session after native
/// pointer capture succeeds.
#[derive(Clone)]
pub struct OpenGpuiNodeDragHandle {
    instance: NodeInstanceKey,
    availability: Rc<Cell<OpenGpuiNodeDragAvailability>>,
    activity: OpenGpuiNodeActivityRegistry,
    interactions: OpenGpuiNativeInteractionHandle,
    listener: OpenGpuiSceneGestureListener,
    mount: OpenGpuiMountCapability,
    scope: OpenGpuiNodeDragScope,
}

#[derive(Clone, Copy)]
struct OpenGpuiNodeDragAvailability {
    graph_enabled: bool,
    presentation_enabled: bool,
    dragging: bool,
}

#[derive(Clone, Copy, Default)]
enum OpenGpuiNodeDragScope {
    #[default]
    Native,
    HostPresentation,
}

impl std::fmt::Debug for OpenGpuiNodeDragHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiNodeDragHandle")
            .field("instance", &self.instance)
            .field("interaction_enabled", &self.interaction_enabled())
            .finish_non_exhaustive()
    }
}

impl OpenGpuiNodeDragHandle {
    pub(crate) fn new(
        instance: NodeInstanceKey,
        activity: OpenGpuiNodeActivityRegistry,
        interactions: OpenGpuiNativeInteractionHandle,
        listener: OpenGpuiSceneGestureListener,
        mount: OpenGpuiMountCapability,
    ) -> Self {
        Self {
            instance,
            availability: Rc::new(Cell::new(OpenGpuiNodeDragAvailability {
                graph_enabled: true,
                presentation_enabled: true,
                dragging: false,
            })),
            activity,
            interactions,
            listener,
            mount,
            scope: OpenGpuiNodeDragScope::Native,
        }
    }

    /// Returns the exact retained node instance owned by this capability.
    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    /// Returns whether this handle can currently arm native pointer movement.
    #[must_use]
    pub fn interaction_enabled(&self) -> bool {
        let availability = self.availability.get();
        availability.graph_enabled
            && (availability.presentation_enabled
                || matches!(self.scope, OpenGpuiNodeDragScope::HostPresentation))
            && self.mount.is_live()
    }

    pub(crate) fn set_graph_enabled(&self, enabled: bool) {
        let mut availability = self.availability.get();
        availability.graph_enabled = enabled;
        self.availability.set(availability);
    }

    pub(crate) fn set_presentation_enabled(&self, enabled: bool) {
        let mut availability = self.availability.get();
        availability.presentation_enabled = enabled;
        self.availability.set(availability);
    }

    pub(crate) fn set_dragging(&self, dragging: bool) {
        let mut availability = self.availability.get();
        availability.dragging = dragging;
        self.availability.set(availability);
    }

    pub(crate) fn for_host_presentation(&self) -> Self {
        let mut handle = self.clone();
        handle.scope = OpenGpuiNodeDragScope::HostPresentation;
        handle
    }

    pub(crate) fn overlay_boundary(&self) -> OpenGpuiOverlayInteractionBoundary {
        OpenGpuiOverlayInteractionBoundary::new(self.interactions.clone())
    }

    /// Wraps an ordinary Open GPUI element as the only pointer surface that may drag this node.
    pub fn wrap<E>(&self, child: E) -> impl IntoElement + use<E>
    where
        E: IntoElement,
    {
        OpenGpuiNodeDragHandleElement {
            child: Some(child.into_any_element()),
            handle: self.clone(),
        }
    }
}

struct OpenGpuiNodeDragHandleElement {
    child: Option<AnyElement>,
    handle: OpenGpuiNodeDragHandle,
}

impl Element for OpenGpuiNodeDragHandleElement {
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
        let mut child = self.child.take().expect("node drag handle child missing");
        let layout = child.request_layout(window, cx);
        (layout, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<open_gpui::Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<open_gpui::Pixels>,
        child: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let enabled = self.handle.interaction_enabled();
        if enabled {
            let cursor = if self.handle.availability.get().dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            };
            window.set_cursor_style(cursor, hitbox);
            let hitbox = hitbox.clone();
            let handle = self.handle.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !hitbox.is_hovered(window)
                    || window.default_prevented()
                    || !handle.interaction_enabled()
                {
                    return;
                }
                if handle.interactions.begin(
                    OpenGpuiSceneGestureKind::Node(handle.instance.clone()),
                    MouseButton::Left,
                    handle.listener.clone(),
                    Some(handle.activity.clone()),
                    event,
                    window,
                    cx,
                ) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
        }
        child.paint(window, cx);
    }
}

impl IntoElement for OpenGpuiNodeDragHandleElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
