use std::cell::{Cell, RefCell};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    geometry::ViewPoint, geometry::ViewRect, geometry::ViewSize, geometry::side_anchor,
    graph::PortInstanceKey, view::FrameNode, view::NodeInstanceKey, view::NodeLayout,
    view::NodeLayoutBuilderError,
};
use open_gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, Div, Element, ElementId, EntityId,
    FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement,
    IntoElement, KeyUpEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Role, Stateful, StatefulInteractiveElement, Window, div, point, px,
    size,
};

use crate::accessibility::{
    OPEN_GPUI_INTERACTIVE_PORT_ROLE, OPEN_GPUI_STATIC_PORT_ROLE, open_gpui_port_element_id,
    open_gpui_port_measurement_element_id,
};
use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::OpenGpuiMountCapability;
use crate::interaction::{
    OpenGpuiNativeInteractionHandle, OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragOwner, OpenGpuiPortDragPhase, OpenGpuiPortEvent,
    OpenGpuiPortListener, port_drag_owner,
};
use crate::presentation::OpenGpuiPortInteractionState;
use crate::theme::OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER;

pub(crate) type OpenGpuiHostPortListener =
    Rc<dyn Fn(&OpenGpuiPortEvent, Option<OpenGpuiPortListener>, &mut Window, &mut App) + 'static>;
pub(crate) type OpenGpuiHostPortHoverListener =
    Rc<dyn Fn(&NodeInstanceKey, PortInstanceKey, bool, &mut Window, &mut App) + 'static>;

/// Edge-routing side associated with one native port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiPortSide {
    Top,
    Right,
    Bottom,
    Left,
}

/// Capability identifying the sole graph-interactive node presentation for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenGpuiGraphPresentationSource {
    Detail,
    Overview,
}

/// Route-anchor policy for a native port marker.
///
/// Anchors are resolved from the measured visual bounds. `Offset` is relative to the visual
/// center and uses Open GPUI logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum OpenGpuiPortAnchor {
    /// Routes edges through the center of the measured marker.
    #[default]
    Center,
    /// Routes edges through the midpoint of the marker side selected by [`OpenGpuiPortSide`].
    Boundary,
    /// Routes edges through a logical-pixel offset from the measured marker center.
    Offset(ViewPoint),
}

impl OpenGpuiPortSide {
    fn for_direction(direction: PortDirection) -> Self {
        match direction {
            PortDirection::In => Self::Left,
            PortDirection::Out => Self::Right,
        }
    }

    fn handle_position(self) -> HandlePosition {
        match self {
            Self::Top => HandlePosition::Top,
            Self::Right => HandlePosition::Right,
            Self::Bottom => HandlePosition::Bottom,
            Self::Left => HandlePosition::Left,
        }
    }
}

/// Stable semantic wrapper for one real graph port inside a retained native node.
///
/// The handle intentionally stores no frame token or layout target. Its measured listener writes
/// raw bounds through a weak link to the host's current pending frame.
#[derive(Clone)]
pub struct OpenGpuiPortHandle {
    core: Rc<OpenGpuiPortHandleCore>,
    accessible_name: Option<Arc<str>>,
    side: OpenGpuiPortSide,
    interaction_enabled: bool,
    presentation: OpenGpuiPortInteractionState,
    focus_handle: Option<FocusHandle>,
    anchor: OpenGpuiPortAnchor,
    minimum_hit_diameter: f32,
    source: OpenGpuiGraphPresentationSource,
}

struct OpenGpuiPortHandleCore {
    instance: NodeInstanceKey,
    port_instance: PortInstanceKey,
    default_accessible_name: Arc<str>,
    sink: OpenGpuiMeasurementSink,
}

impl std::fmt::Debug for OpenGpuiPortHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiPortHandle")
            .field("instance", &self.core.instance)
            .field("port_instance", &self.core.port_instance)
            .field("side", &self.side)
            .field("accessible_name", &self.accessible_name())
            .field("interaction_enabled", &self.interaction_enabled)
            .field("presentation", &self.presentation)
            .field("has_focus_handle", &self.focus_handle.is_some())
            .field("anchor", &self.anchor)
            .field("minimum_hit_diameter", &self.minimum_hit_diameter)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiPortHandle {
    pub(crate) fn new(
        instance: NodeInstanceKey,
        port_instance: PortInstanceKey,
        direction: PortDirection,
        name: Arc<str>,
        sink: OpenGpuiMeasurementSink,
    ) -> Self {
        let direction_name = match direction {
            PortDirection::In => "input",
            PortDirection::Out => "output",
        };
        Self {
            core: Rc::new(OpenGpuiPortHandleCore {
                instance,
                port_instance,
                default_accessible_name: Arc::from(format!("{name} {direction_name} port")),
                sink,
            }),
            accessible_name: None,
            side: OpenGpuiPortSide::for_direction(direction),
            interaction_enabled: true,
            presentation: OpenGpuiPortInteractionState::idle(),
            focus_handle: None,
            anchor: OpenGpuiPortAnchor::default(),
            minimum_hit_diameter: OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
            source: OpenGpuiGraphPresentationSource::Detail,
        }
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.core.instance
    }

    #[must_use]
    pub fn port(&self) -> PortId {
        self.core.port_instance.port()
    }

    /// Returns the full runtime identity of this continuous real-port lifetime.
    #[must_use]
    pub fn port_instance(&self) -> PortInstanceKey {
        self.core.port_instance
    }

    #[must_use]
    pub fn side(&self) -> OpenGpuiPortSide {
        self.side
    }

    /// Returns the name exposed to assistive technology for this real graph port.
    #[must_use]
    pub fn accessible_name(&self) -> &str {
        self.accessible_name
            .as_deref()
            .unwrap_or(&self.core.default_accessible_name)
    }

    #[cfg(test)]
    pub(crate) fn shares_core_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.core, &other.core)
    }

    /// Replaces the semantic port-key fallback with an application-facing accessible name.
    #[must_use]
    pub fn with_accessible_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        if !name.trim().is_empty() {
            self.accessible_name = Some(Arc::from(name));
        }
        self
    }

    /// Returns whether this handle accepts native pointer, keyboard, and accessibility input.
    #[must_use]
    pub fn interaction_enabled(&self) -> bool {
        self.interaction_enabled
    }

    /// Enables or disables adapter-local input without changing runtime connection policy.
    #[must_use]
    pub fn with_interaction_enabled(mut self, enabled: bool) -> Self {
        self.interaction_enabled = enabled;
        self
    }

    /// Returns the latest immutable adapter-local interaction state for this real port.
    #[must_use]
    pub fn presentation(&self) -> &OpenGpuiPortInteractionState {
        &self.presentation
    }

    pub(crate) fn with_presentation_state(
        mut self,
        presentation: OpenGpuiPortInteractionState,
    ) -> Self {
        self.presentation = presentation;
        self
    }

    pub(crate) fn with_focus_handle(mut self, focus_handle: Option<FocusHandle>) -> Self {
        self.focus_handle = focus_handle;
        self
    }

    #[must_use]
    pub fn with_side(mut self, side: OpenGpuiPortSide) -> Self {
        self.side = side;
        self
    }

    /// Returns the route-anchor policy resolved from the measured visual bounds.
    #[must_use]
    pub fn anchor(&self) -> OpenGpuiPortAnchor {
        self.anchor
    }

    /// Selects how the route anchor is resolved from the measured visual bounds.
    #[must_use]
    pub fn with_anchor(mut self, anchor: OpenGpuiPortAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Returns the minimum pointer target diameter in logical pixels.
    #[must_use]
    pub fn minimum_hit_diameter(&self) -> f32 {
        self.minimum_hit_diameter
    }

    /// Sets the minimum pointer target diameter around the measured visual element.
    ///
    /// Non-finite and non-positive values are ignored. A marker larger than this diameter keeps
    /// its measured dimensions.
    #[must_use]
    pub fn with_minimum_hit_diameter(mut self, diameter: f32) -> Self {
        if diameter.is_finite() && diameter > 0.0 {
            self.minimum_hit_diameter = diameter;
        }
        self
    }

    pub(crate) fn for_host_overview(mut self) -> Self {
        self.source = OpenGpuiGraphPresentationSource::Overview;
        self
    }

    /// Wraps an ordinary Open GPUI element and reports its post-layout bounds as this real port.
    pub fn measure(&self, child: impl IntoElement) -> AnyElement {
        let child = child.into_any_element();
        OpenGpuiPortMeasurementElement {
            id: open_gpui_port_element_id(self.core.sink.host, &self.core.instance, self.port()),
            child: Some(child),
            wrapper: self.core.sink.allocate_port_wrapper(),
            port_instance: self.core.port_instance,
            side: self.side,
            anchor: self.anchor,
            minimum_hit_diameter: self.minimum_hit_diameter,
            accessible_name: self.semantic_accessible_name(),
            source: self.source,
            sink: self.core.sink.clone(),
        }
        .into_any_element()
    }

    /// Wraps a native port marker with host-owned graph interaction and accessibility behavior.
    pub fn interactive(&self, child: impl IntoElement) -> AnyElement {
        self.interactive_inner(child, None)
    }

    /// Wraps a native port marker and observes host-owned graph events while this mount is current.
    ///
    /// The host reducer always runs first. If it retires the mount, the observer is not called.
    pub fn interactive_with_observer(
        &self,
        child: impl IntoElement,
        observer: impl Fn(OpenGpuiPortEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.interactive_inner(child, Some(Rc::new(observer)))
    }

    fn interactive_inner(
        &self,
        child: impl IntoElement,
        observer: Option<OpenGpuiPortListener>,
    ) -> AnyElement {
        let child = child.into_any_element();
        if !self
            .core
            .sink
            .admits_port(self.core.port_instance, self.source)
        {
            return child;
        }
        let interaction_enabled = self.interaction_enabled
            && !self.presentation.disabled()
            && self.core.sink.mount.is_live();
        let semantic_id =
            open_gpui_port_element_id(self.core.sink.host, &self.core.instance, self.port());
        let measurement_id = open_gpui_port_measurement_element_id(
            self.core.sink.host,
            &self.core.instance,
            self.port(),
        );
        let Some(owner) = port_drag_owner(self.core.instance.clone(), self.core.port_instance)
        else {
            return child;
        };
        let host_listener = self.core.sink.host_port_listener.clone();
        let mount = self.core.sink.mount.clone();
        let listener: OpenGpuiPortListener = Rc::new(move |event, window, cx| {
            if !mount.is_live() && !is_retirement_cancellation(&event) {
                return;
            }
            let observer = if is_retirement_cancellation(&event) {
                None
            } else {
                observer.clone()
            };
            host_listener(&event, observer, window, cx);
        });
        let keyboard_listener = listener.clone();
        let keyboard_owner = owner.clone();
        let accessibility_listener = listener.clone();
        let accessibility_owner = owner.clone();
        let focus_handle = self
            .focus_handle
            .as_ref()
            .map(|focus_handle| focus_handle.clone().tab_stop(interaction_enabled));
        let child = interactive_port_semantic_element(
            semantic_id,
            self.semantic_accessible_name(),
            interaction_enabled,
            focus_handle.as_ref(),
            child,
            move |event: &KeyUpEvent, window, cx| {
                let keystroke = &event.keystroke;
                if !keystroke.modifiers.modified()
                    && (keystroke.key.eq_ignore_ascii_case("enter")
                        || keystroke.key.eq_ignore_ascii_case("space"))
                {
                    keyboard_listener(
                        OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
                            keyboard_owner.clone(),
                            OpenGpuiPortActivationSource::Keyboard,
                        )),
                        window,
                        cx,
                    );
                    window.prevent_default();
                    cx.stop_propagation();
                }
            },
            move |_, window, cx| {
                accessibility_listener(
                    OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
                        accessibility_owner.clone(),
                        OpenGpuiPortActivationSource::Accessibility,
                    )),
                    window,
                    cx,
                );
            },
        );
        OpenGpuiPortInteractionElement {
            id: measurement_id,
            child: Some(child.into_any_element()),
            wrapper: self.core.sink.allocate_port_wrapper(),
            owner,
            port_instance: self.core.port_instance,
            side: self.side,
            anchor: self.anchor,
            interaction_enabled,
            minimum_hit_diameter: self.minimum_hit_diameter,
            source: self.source,
            sink: self.core.sink.clone(),
            activity: self.core.sink.activity.clone(),
            interactions: self.core.sink.interactions.clone(),
            listener,
        }
        .into_any_element()
    }

    fn semantic_accessible_name(&self) -> String {
        format!(
            "{}, {}",
            self.accessible_name(),
            self.presentation.accessible_state_text()
        )
    }
}

fn is_retirement_cancellation(event: &OpenGpuiPortEvent) -> bool {
    matches!(
        event,
        OpenGpuiPortEvent::Pointer(event)
            if matches!(
                event.phase(),
                OpenGpuiPortDragPhase::Cancel(
                    OpenGpuiPortDragCancelReason::OwnerRetired
                        | OpenGpuiPortDragCancelReason::HostClosed
                )
            )
    )
}

fn interactive_port_semantic_element(
    id: ElementId,
    accessible_name: String,
    enabled: bool,
    focus_handle: Option<&FocusHandle>,
    child: impl IntoElement,
    on_key_up: impl Fn(&KeyUpEvent, &mut Window, &mut App) + 'static,
    on_a11y_click: impl FnMut(Option<&open_gpui::accesskit::ActionData>, &mut Window, &mut App)
    + 'static,
) -> Stateful<Div> {
    let element = div()
        .id(id)
        .role(OPEN_GPUI_INTERACTIVE_PORT_ROLE)
        .aria_label(accessible_name)
        .aria_disabled(!enabled)
        .child(child);
    let mut element = match focus_handle {
        Some(focus_handle) => element.track_focus(focus_handle),
        None => element.focusable(),
    }
    .tab_stop(enabled);
    if enabled {
        element = element
            .on_key_up(on_key_up)
            .on_a11y_action(open_gpui::accesskit::Action::Click, on_a11y_click);
    }
    element
}

fn write_static_port_accessibility(node: &mut open_gpui::accesskit::Node, name: &str) {
    node.set_label(name.to_owned());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenGpuiPortWrapperId(u64);

struct OpenGpuiPortMeasurementElement {
    id: ElementId,
    child: Option<AnyElement>,
    wrapper: OpenGpuiPortWrapperId,
    port_instance: PortInstanceKey,
    side: OpenGpuiPortSide,
    anchor: OpenGpuiPortAnchor,
    minimum_hit_diameter: f32,
    accessible_name: String,
    source: OpenGpuiGraphPresentationSource,
    sink: OpenGpuiMeasurementSink,
}

impl Element for OpenGpuiPortMeasurementElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        let mut child = self.child.take().expect("port measurement child missing");
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
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
        child.prepaint(window, cx);
        let hit_bounds = minimum_hit_pixel_bounds(bounds, self.minimum_hit_diameter);
        let interaction_bounds =
            clipped_interaction_pixel_bounds(bounds, hit_bounds, window.content_mask().bounds);
        let _ = self.sink.record_port(
            self.source,
            self.wrapper,
            self.port_instance,
            bounds,
            hit_bounds,
            interaction_bounds,
            self.side,
            self.anchor,
        );
        if interaction_bounds.is_none() {
            self.sink.clear_port_hover(self.port_instance, window, cx);
        }
        interaction_bounds.map(|bounds| window.insert_hitbox(bounds, HitboxBehavior::Normal))
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
        if let Some(hitbox) = hitbox {
            self.sink
                .reconcile_port_hover(self.port_instance, hitbox, window, cx);
            self.sink
                .track_port_hover(self.port_instance, hitbox.clone(), window);
        }
        child.paint(window, cx);
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(OPEN_GPUI_STATIC_PORT_ROLE)
    }

    fn write_a11y_info(&self, node: &mut open_gpui::accesskit::Node) {
        write_static_port_accessibility(node, &self.accessible_name);
    }
}

impl IntoElement for OpenGpuiPortMeasurementElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct OpenGpuiPortInteractionElement {
    id: ElementId,
    child: Option<AnyElement>,
    wrapper: OpenGpuiPortWrapperId,
    owner: OpenGpuiPortDragOwner,
    port_instance: PortInstanceKey,
    side: OpenGpuiPortSide,
    anchor: OpenGpuiPortAnchor,
    interaction_enabled: bool,
    minimum_hit_diameter: f32,
    source: OpenGpuiGraphPresentationSource,
    sink: OpenGpuiMeasurementSink,
    activity: OpenGpuiNodeActivityRegistry,
    interactions: OpenGpuiNativeInteractionHandle,
    listener: OpenGpuiPortListener,
}

impl Element for OpenGpuiPortInteractionElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        let mut child = self.child.take().expect("port drag element child missing");
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
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
        child.prepaint(window, cx);
        let hit_bounds = minimum_hit_pixel_bounds(bounds, self.minimum_hit_diameter);
        let interaction_bounds =
            clipped_interaction_pixel_bounds(bounds, hit_bounds, window.content_mask().bounds);
        let _ = self.sink.record_port(
            self.source,
            self.wrapper,
            self.port_instance,
            bounds,
            hit_bounds,
            interaction_bounds,
            self.side,
            self.anchor,
        );
        if interaction_bounds.is_none() {
            self.sink
                .clear_port_hover(self.owner.port_instance(), window, cx);
        }
        let hitbox = interaction_bounds
            .map(|bounds| window.insert_hitbox(bounds, HitboxBehavior::BlockMouse));
        if self.sink.mount.is_live() {
            if self.interaction_enabled {
                if let Some(hitbox) = hitbox.as_ref() {
                    self.interactions.rebind_port(
                        &self.owner,
                        &self.sink.mount,
                        hitbox.id,
                        window,
                        cx,
                    );
                }
            } else {
                self.interactions
                    .disable_port_deferred(&self.owner, &self.sink.mount, window, cx);
            }
        }
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
        let Some(hitbox) = hitbox.as_ref() else {
            child.paint(window, cx);
            return;
        };
        self.sink
            .reconcile_port_hover(self.owner.port_instance(), hitbox, window, cx);
        self.sink
            .track_port_hover(self.owner.port_instance(), hitbox.clone(), window);
        if !self.interaction_enabled {
            let disabled_hitbox = hitbox.clone();
            window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && disabled_hitbox.is_hovered(window) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
            let disabled_hitbox = hitbox.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && disabled_hitbox.is_hovered(window) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
            child.paint(window, cx);
            return;
        }

        let hitbox = hitbox.clone();
        window.set_cursor_style(CursorStyle::Crosshair, &hitbox);
        let owner = self.owner.clone();
        let listener = self.listener.clone();
        let activity = self.activity.clone();
        let mount = self.sink.mount.clone();
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
                && interactions.begin_port_drag(
                    owner.clone(),
                    mount.clone(),
                    listener.clone(),
                    activity.clone(),
                    hitbox.id,
                    event,
                    window,
                    cx,
                )
            {
                window.prevent_default();
                cx.stop_propagation();
            }
        });
        child.paint(window, cx);
    }
}

impl IntoElement for OpenGpuiPortInteractionElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct OpenGpuiContentMeasurementElement {
    child: Option<AnyElement>,
    sink: OpenGpuiMeasurementSink,
}

impl Element for OpenGpuiContentMeasurementElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

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
        let mut child = self
            .child
            .take()
            .expect("content measurement child missing");
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.prepaint(window, cx);
        let _ = self.sink.record_content(bounds);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
    }
}

impl IntoElement for OpenGpuiContentMeasurementElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn minimum_hit_pixel_bounds(bounds: Bounds<Pixels>, diameter: f32) -> Bounds<Pixels> {
    if !diameter.is_finite() || diameter <= 0.0 {
        return bounds;
    }
    let width = bounds.size.width.as_f32().max(diameter);
    let height = bounds.size.height.as_f32().max(diameter);
    let x = bounds.origin.x.as_f32() - (width - bounds.size.width.as_f32()) * 0.5;
    let y = bounds.origin.y.as_f32() - (height - bounds.size.height.as_f32()) * 0.5;
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width), px(height)),
    }
}

fn clipped_interaction_pixel_bounds(
    visual_bounds: Bounds<Pixels>,
    hit_bounds: Bounds<Pixels>,
    content_bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    let visible = visual_bounds.intersect(&content_bounds);
    if visible.size.width.as_f32() <= 0.0 || visible.size.height.as_f32() <= 0.0 {
        return None;
    }
    let interaction = hit_bounds.intersect(&content_bounds);
    (interaction.size.width.as_f32() > 0.0 && interaction.size.height.as_f32() > 0.0)
        .then_some(interaction)
}

#[derive(Clone)]
pub(crate) struct OpenGpuiMeasurementSink {
    host: EntityId,
    instance: NodeInstanceKey,
    pending: Rc<RefCell<Option<Weak<OpenGpuiPendingMeasurements>>>>,
    errors: Rc<RefCell<Vec<OpenGpuiMeasurementError>>>,
    error_queue_busy: Rc<Cell<bool>>,
    activity: OpenGpuiNodeActivityRegistry,
    interactions: OpenGpuiNativeInteractionHandle,
    host_port_listener: OpenGpuiHostPortListener,
    host_port_hover_listener: OpenGpuiHostPortHoverListener,
    hovered_port: Rc<Cell<Option<PortInstanceKey>>>,
    next_port_wrapper: Rc<Cell<u64>>,
    mount: OpenGpuiMountCapability,
}

impl std::fmt::Debug for OpenGpuiMeasurementSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiMeasurementSink")
            .field("instance", &self.instance)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiMeasurementSink {
    pub(crate) fn new(
        host: EntityId,
        instance: NodeInstanceKey,
        activity: OpenGpuiNodeActivityRegistry,
        interactions: OpenGpuiNativeInteractionHandle,
        host_port_listener: OpenGpuiHostPortListener,
        host_port_hover_listener: OpenGpuiHostPortHoverListener,
        mount: OpenGpuiMountCapability,
    ) -> Self {
        Self {
            host,
            instance,
            pending: Rc::new(RefCell::new(None)),
            errors: Rc::new(RefCell::new(Vec::new())),
            error_queue_busy: Rc::new(Cell::new(false)),
            activity,
            interactions,
            host_port_listener,
            host_port_hover_listener,
            hovered_port: Rc::new(Cell::new(None)),
            next_port_wrapper: Rc::new(Cell::new(0)),
            mount,
        }
    }

    fn allocate_port_wrapper(&self) -> OpenGpuiPortWrapperId {
        let current = self.next_port_wrapper.get();
        self.next_port_wrapper.set(
            current
                .checked_add(1)
                .expect("native port wrapper identity space exhausted"),
        );
        OpenGpuiPortWrapperId(current)
    }

    fn track_port_hover(&self, port: PortInstanceKey, hitbox: Hitbox, window: &mut Window) {
        let hovered_port = self.hovered_port.clone();
        let listener = self.host_port_hover_listener.clone();
        let instance = self.instance.clone();
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if hitbox.is_hovered(window) {
                if hovered_port.get() == Some(port) {
                    return;
                }
                hovered_port.set(Some(port));
                listener(&instance, port, true, window, cx);
            } else if hovered_port.get() == Some(port) {
                hovered_port.set(None);
                listener(&instance, port, false, window, cx);
            }
        });
    }

    fn clear_port_hover(&self, port: PortInstanceKey, window: &mut Window, cx: &mut App) {
        if self.hovered_port.get() != Some(port) {
            return;
        }
        self.hovered_port.set(None);
        (self.host_port_hover_listener)(&self.instance, port, false, window, cx);
    }

    fn reconcile_port_hover(
        &self,
        port: PortInstanceKey,
        hitbox: &Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !hitbox.is_hovered(window) {
            self.clear_port_hover(port, window, cx);
        }
    }

    pub(crate) fn activate(
        &self,
        pending: &Rc<OpenGpuiPendingMeasurements>,
        source: OpenGpuiGraphPresentationSource,
        content_epoch: u64,
    ) -> Result<(), OpenGpuiMeasurementError> {
        if !self.mount.is_live() {
            return Err(OpenGpuiMeasurementError::Inactive {
                node: self.instance.node(),
            });
        }
        let mut current =
            self.pending
                .try_borrow_mut()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(self.instance.node()),
                })?;
        self.next_port_wrapper.set(0);
        pending.activate(&self.instance, source, content_epoch)?;
        *current = Some(Rc::downgrade(pending));
        Ok(())
    }

    fn admits_port(&self, port: PortInstanceKey, source: OpenGpuiGraphPresentationSource) -> bool {
        self.mount.is_live()
            && self
                .pending
                .try_borrow()
                .ok()
                .and_then(|pending| pending.as_ref().and_then(Weak::upgrade))
                .is_some_and(|pending| pending.admits_port(&self.instance, port, source))
    }

    pub(crate) fn measure_content(&self, child: impl IntoElement) -> AnyElement {
        let child = child.into_any_element();
        if !self.mount.is_live() {
            return child;
        }
        OpenGpuiContentMeasurementElement {
            child: Some(child),
            sink: self.clone(),
        }
        .into_any_element()
    }

    fn record_content(&self, bounds: Bounds<Pixels>) -> Result<(), OpenGpuiMeasurementError> {
        if !self.mount.is_live() {
            return Err(OpenGpuiMeasurementError::Inactive {
                node: self.instance.node(),
            });
        }
        let result: Result<(), OpenGpuiMeasurementError> = (|| {
            let pending = self
                .pending
                .try_borrow()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(self.instance.node()),
                })?
                .as_ref()
                .and_then(Weak::upgrade)
                .ok_or(OpenGpuiMeasurementError::Inactive {
                    node: self.instance.node(),
                })?;
            pending.record_content(
                &self.instance,
                CanvasSize {
                    width: bounds.size.width.as_f32(),
                    height: bounds.size.height.as_f32(),
                },
            )
        })();
        if let Err(error) = &result {
            self.record_error(error.clone());
        }
        result
    }

    fn record_port(
        &self,
        source: OpenGpuiGraphPresentationSource,
        wrapper: OpenGpuiPortWrapperId,
        port_instance: PortInstanceKey,
        bounds: Bounds<Pixels>,
        hit_bounds: Bounds<Pixels>,
        interaction_bounds: Option<Bounds<Pixels>>,
        side: OpenGpuiPortSide,
        anchor: OpenGpuiPortAnchor,
    ) -> Result<(), OpenGpuiMeasurementError> {
        if !self.mount.is_live() {
            return Err(OpenGpuiMeasurementError::Inactive {
                node: self.instance.node(),
            });
        }
        let result: Result<(), OpenGpuiMeasurementError> = (|| {
            let pending = self
                .pending
                .try_borrow()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(self.instance.node()),
                })?
                .as_ref()
                .and_then(Weak::upgrade)
                .ok_or(OpenGpuiMeasurementError::Inactive {
                    node: self.instance.node(),
                })?;
            let visual_bounds = view_rect(bounds);
            let outward = side.handle_position();
            let route_anchor = resolve_route_anchor(visual_bounds, outward, anchor).ok_or(
                OpenGpuiMeasurementError::InvalidPortBounds {
                    node: self.instance.node(),
                    port: port_instance.port(),
                },
            )?;
            pending.record_port(
                &self.instance,
                source,
                wrapper,
                RawPortMeasurement {
                    port_instance,
                    visual_bounds,
                    hit_bounds: view_rect(hit_bounds),
                    interaction_bounds: interaction_bounds.map(view_rect),
                    route_anchor,
                    outward,
                },
            )
        })();
        if let Err(error) = &result {
            self.record_error(error.clone());
        }
        result
    }

    pub(crate) fn take_errors(&self) -> Vec<OpenGpuiMeasurementError> {
        let mut errors = self
            .errors
            .try_borrow_mut()
            .map(|mut errors| std::mem::take(&mut *errors))
            .unwrap_or_else(|_| {
                vec![OpenGpuiMeasurementError::Busy {
                    node: Some(self.instance.node()),
                }]
            });
        if self.error_queue_busy.replace(false) {
            errors.push(OpenGpuiMeasurementError::Busy {
                node: Some(self.instance.node()),
            });
        }
        errors
    }

    fn record_error(&self, error: OpenGpuiMeasurementError) {
        if let Ok(mut errors) = self.errors.try_borrow_mut() {
            errors.push(error);
        } else {
            self.error_queue_busy.set(true);
        }
    }
}

struct PendingNodeMeasurement {
    frame_node: FrameNode,
    expected_ports: BTreeMap<PortId, PortInstanceKey>,
    source: Option<OpenGpuiGraphPresentationSource>,
    content_epoch: u64,
    root: Option<ViewRect>,
    ports: BTreeMap<PortId, RecordedPortMeasurement>,
    duplicate_ports: BTreeSet<PortId>,
    content_size: Option<CanvasSize>,
    has_foreign_port: bool,
}

struct RawPortMeasurement {
    port_instance: PortInstanceKey,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    interaction_bounds: Option<ViewRect>,
    route_anchor: ViewPoint,
    outward: HandlePosition,
}

struct RecordedPortMeasurement {
    wrapper: OpenGpuiPortWrapperId,
    measurement: RawPortMeasurement,
}

pub(crate) struct OpenGpuiPendingMeasurements {
    order: Vec<NodeInstanceKey>,
    nodes: RefCell<BTreeMap<NodeInstanceKey, PendingNodeMeasurement>>,
    transform: ViewportTransform,
    presentation_offsets: BTreeMap<NodeId, CanvasPoint>,
    scene_origin: Cell<Option<ViewPoint>>,
    environment_epoch: u64,
    errors: RefCell<Vec<OpenGpuiMeasurementError>>,
    error_queue_busy: Cell<bool>,
}

pub(crate) struct OpenGpuiSealedMeasurements {
    layouts: Vec<NodeLayout>,
    errors: Vec<OpenGpuiMeasurementError>,
}

impl OpenGpuiSealedMeasurements {
    pub(crate) fn into_parts(self) -> (Vec<NodeLayout>, Vec<OpenGpuiMeasurementError>) {
        (self.layouts, self.errors)
    }
}

impl OpenGpuiPendingMeasurements {
    pub(crate) fn new(
        frame: &[FrameNode],
        transform: ViewportTransform,
        presentation_offsets: BTreeMap<NodeId, CanvasPoint>,
        environment_epoch: u64,
    ) -> Self {
        let order = frame.iter().map(|node| node.instance().clone()).collect();
        let nodes = frame
            .iter()
            .map(|node| {
                (
                    node.instance().clone(),
                    PendingNodeMeasurement {
                        frame_node: node.clone(),
                        expected_ports: node
                            .ports()
                            .iter()
                            .map(|port| (port.id(), port.instance()))
                            .collect(),
                        source: None,
                        content_epoch: 0,
                        root: None,
                        ports: BTreeMap::new(),
                        duplicate_ports: BTreeSet::new(),
                        content_size: None,
                        has_foreign_port: false,
                    },
                )
            })
            .collect();
        Self {
            order,
            nodes: RefCell::new(nodes),
            transform,
            presentation_offsets,
            scene_origin: Cell::new(None),
            environment_epoch,
            errors: RefCell::new(Vec::new()),
            error_queue_busy: Cell::new(false),
        }
    }

    pub(crate) fn record_scene(&self, bounds: Bounds<Pixels>) {
        self.scene_origin.set(Some(view_rect(bounds).origin));
    }

    pub(crate) fn presentation_offsets(&self) -> &BTreeMap<NodeId, CanvasPoint> {
        &self.presentation_offsets
    }

    pub(crate) fn presentation_offset(&self, node: NodeId) -> Option<CanvasPoint> {
        self.presentation_offsets.get(&node).copied()
    }

    pub(crate) fn record_root(
        &self,
        instance: &NodeInstanceKey,
        source: OpenGpuiGraphPresentationSource,
        bounds: Bounds<Pixels>,
    ) -> Result<(), OpenGpuiMeasurementError> {
        let result: Result<(), OpenGpuiMeasurementError> = (|| {
            let mut nodes =
                self.nodes
                    .try_borrow_mut()
                    .map_err(|_| OpenGpuiMeasurementError::Busy {
                        node: Some(instance.node()),
                    })?;
            let node =
                nodes
                    .get_mut(instance)
                    .ok_or(OpenGpuiMeasurementError::UnknownInstance {
                        node: instance.node(),
                    })?;
            if node.source != Some(source) {
                return Ok(());
            }
            node.root = Some(view_rect(bounds));
            Ok(())
        })();
        if let Err(error) = &result {
            self.record_error(error.clone());
        }
        result
    }

    fn record_port(
        &self,
        instance: &NodeInstanceKey,
        source: OpenGpuiGraphPresentationSource,
        wrapper: OpenGpuiPortWrapperId,
        measurement: RawPortMeasurement,
    ) -> Result<(), OpenGpuiMeasurementError> {
        let mut nodes =
            self.nodes
                .try_borrow_mut()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(instance.node()),
                })?;
        let node = nodes
            .get_mut(instance)
            .ok_or(OpenGpuiMeasurementError::UnknownInstance {
                node: instance.node(),
            })?;
        if node.source != Some(source) {
            return Ok(());
        }
        let port = measurement.port_instance.port();
        if node.expected_ports.get(&port).copied() != Some(measurement.port_instance) {
            node.has_foreign_port = true;
            return Err(OpenGpuiMeasurementError::UnknownPort {
                node: instance.node(),
                port,
            });
        }
        match node.ports.entry(port) {
            Entry::Vacant(entry) => {
                entry.insert(RecordedPortMeasurement {
                    wrapper,
                    measurement,
                });
            }
            Entry::Occupied(mut entry) if entry.get().wrapper == wrapper => {
                entry.get_mut().measurement = measurement;
            }
            Entry::Occupied(_) => {
                node.duplicate_ports.insert(port);
            }
        }
        Ok(())
    }

    fn activate(
        &self,
        instance: &NodeInstanceKey,
        source: OpenGpuiGraphPresentationSource,
        content_epoch: u64,
    ) -> Result<(), OpenGpuiMeasurementError> {
        let mut nodes =
            self.nodes
                .try_borrow_mut()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(instance.node()),
                })?;
        let node = nodes
            .get_mut(instance)
            .ok_or(OpenGpuiMeasurementError::UnknownInstance {
                node: instance.node(),
            })?;
        node.source = Some(source);
        node.content_epoch = content_epoch;
        Ok(())
    }

    fn admits_port(
        &self,
        instance: &NodeInstanceKey,
        port: PortInstanceKey,
        source: OpenGpuiGraphPresentationSource,
    ) -> bool {
        self.nodes
            .try_borrow()
            .ok()
            .and_then(|nodes| {
                let node = nodes.get(instance)?;
                (node.source == Some(source)
                    && node.expected_ports.get(&port.port()).copied() == Some(port))
                .then_some(())
            })
            .is_some()
    }

    fn record_content(
        &self,
        instance: &NodeInstanceKey,
        size: CanvasSize,
    ) -> Result<(), OpenGpuiMeasurementError> {
        if !size.is_positive_finite() {
            return Err(OpenGpuiMeasurementError::InvalidContentBounds {
                node: instance.node(),
            });
        }
        let mut nodes =
            self.nodes
                .try_borrow_mut()
                .map_err(|_| OpenGpuiMeasurementError::Busy {
                    node: Some(instance.node()),
                })?;
        let node = nodes
            .get_mut(instance)
            .ok_or(OpenGpuiMeasurementError::UnknownInstance {
                node: instance.node(),
            })?;
        node.content_size = Some(size);
        Ok(())
    }

    pub(crate) fn environment_epoch(&self) -> u64 {
        self.environment_epoch
    }

    pub(crate) fn content_sizes(&self) -> Vec<(NodeInstanceKey, u64, CanvasSize)> {
        let Ok(nodes) = self.nodes.try_borrow() else {
            return Vec::new();
        };
        self.order
            .iter()
            .filter_map(|instance| {
                let node = nodes.get(instance)?;
                node.content_size
                    .map(|size| (instance.clone(), node.content_epoch, size))
            })
            .collect()
    }

    pub(crate) fn seal_layouts(&self) -> OpenGpuiSealedMeasurements {
        let Some(scene_origin) = self.scene_origin.get() else {
            return OpenGpuiSealedMeasurements {
                layouts: Vec::new(),
                errors: vec![OpenGpuiMeasurementError::MissingSceneRoot],
            };
        };
        let Ok(nodes) = self.nodes.try_borrow() else {
            return OpenGpuiSealedMeasurements {
                layouts: Vec::new(),
                errors: vec![OpenGpuiMeasurementError::Busy { node: None }],
            };
        };
        let mut layouts = Vec::with_capacity(self.order.len());
        let mut errors = Vec::new();
        for instance in &self.order {
            let Some(node) = nodes.get(instance) else {
                errors.push(OpenGpuiMeasurementError::UnknownInstance {
                    node: instance.node(),
                });
                continue;
            };
            if node.has_foreign_port {
                errors.push(OpenGpuiMeasurementError::InvalidMeasuredPortSet {
                    node: instance.node(),
                });
                continue;
            }
            if !node.duplicate_ports.is_empty() {
                errors.push(OpenGpuiMeasurementError::DuplicatePortMeasurements {
                    node: instance.node(),
                    ports: node.duplicate_ports.iter().copied().collect(),
                });
                continue;
            }
            let Some(measured_root) = node.root.map(|root| relative_rect(root, scene_origin))
            else {
                errors.push(OpenGpuiMeasurementError::MissingNodeRoot {
                    node: instance.node(),
                });
                continue;
            };
            let missing_ports = node
                .expected_ports
                .keys()
                .filter(|port| !node.ports.contains_key(port))
                .copied()
                .collect::<Vec<_>>();
            if !missing_ports.is_empty() {
                errors.push(OpenGpuiMeasurementError::MissingPortMeasurements {
                    node: instance.node(),
                    ports: missing_ports,
                });
                continue;
            }
            let presentation_offset = self
                .presentation_offsets
                .get(&instance.node())
                .map(|offset| {
                    ViewPoint::new(
                        offset.x * self.transform.zoom,
                        offset.y * self.transform.zoom,
                    )
                })
                .unwrap_or_default();
            let measured_root = remove_presentation_offset(measured_root, presentation_offset);
            let Some(authoritative_origin) =
                authoritative_node_origin(&node.frame_node, self.transform)
            else {
                errors.push(OpenGpuiMeasurementError::InvalidContentBounds {
                    node: instance.node(),
                });
                continue;
            };
            let root_alignment = ViewPoint::new(
                authoritative_origin.x - measured_root.origin.x,
                authoritative_origin.y - measured_root.origin.y,
            );
            let root = translate_view_rect(measured_root, root_alignment);
            let mut layout = node.frame_node.begin_layout(root);
            let mut invalid = None;
            for port in node.frame_node.ports() {
                let Some(measurement) = node.ports.get(&port.id()).map(|port| &port.measurement)
                else {
                    unreachable!("missing real ports are rejected before layout assembly")
                };
                if let Err(source) = layout.port_rendered_with_interaction_bounds(
                    port.layout_target(),
                    align_measured_rect(
                        measurement.visual_bounds,
                        scene_origin,
                        presentation_offset,
                        root_alignment,
                    ),
                    align_measured_rect(
                        measurement.hit_bounds,
                        scene_origin,
                        presentation_offset,
                        root_alignment,
                    ),
                    measurement.interaction_bounds.map(|bounds| {
                        align_measured_rect(
                            bounds,
                            scene_origin,
                            presentation_offset,
                            root_alignment,
                        )
                    }),
                    align_measured_point(
                        measurement.route_anchor,
                        scene_origin,
                        presentation_offset,
                        root_alignment,
                    ),
                    measurement.outward,
                ) {
                    invalid = Some(OpenGpuiMeasurementError::InvalidPortCoverage {
                        node: instance.node(),
                        source,
                    });
                    break;
                }
            }
            if let Some(error) = invalid {
                errors.push(error);
                continue;
            }
            match layout.finish() {
                Ok(layout) => layouts.push(layout),
                Err(source) => errors.push(OpenGpuiMeasurementError::InvalidPortCoverage {
                    node: instance.node(),
                    source,
                }),
            }
        }
        OpenGpuiSealedMeasurements { layouts, errors }
    }

    pub(crate) fn take_errors(&self) -> Vec<OpenGpuiMeasurementError> {
        let mut errors = self
            .errors
            .try_borrow_mut()
            .map(|mut errors| std::mem::take(&mut *errors))
            .unwrap_or_else(|_| vec![OpenGpuiMeasurementError::Busy { node: None }]);
        if self.error_queue_busy.replace(false) {
            errors.push(OpenGpuiMeasurementError::Busy { node: None });
        }
        errors
    }

    fn record_error(&self, error: OpenGpuiMeasurementError) {
        if let Ok(mut errors) = self.errors.try_borrow_mut() {
            errors.push(error);
        } else {
            self.error_queue_busy.set(true);
        }
    }
}

fn remove_presentation_offset(rect: ViewRect, offset: ViewPoint) -> ViewRect {
    ViewRect::new(
        remove_point_presentation_offset(rect.origin, offset),
        rect.size,
    )
}

fn remove_point_presentation_offset(point: ViewPoint, offset: ViewPoint) -> ViewPoint {
    ViewPoint::new(point.x - offset.x, point.y - offset.y)
}

fn authoritative_node_origin(
    frame_node: &FrameNode,
    transform: ViewportTransform,
) -> Option<ViewPoint> {
    let frame_bounds = frame_node.bounds();
    let canvas_bounds = frame_node
        .node()
        .and_then(|node| {
            let size = node
                .size
                .filter(|size| size.is_positive_finite())
                .unwrap_or(frame_bounds.size);
            node.bounds_for_size(size)
        })
        .unwrap_or(frame_bounds);
    let origin = transform.screen_point_for_canvas(canvas_bounds.origin)?;
    Some(ViewPoint::new(origin.x, origin.y))
}

fn align_measured_rect(
    rect: ViewRect,
    scene_origin: ViewPoint,
    presentation_offset: ViewPoint,
    root_alignment: ViewPoint,
) -> ViewRect {
    translate_view_rect(
        remove_presentation_offset(relative_rect(rect, scene_origin), presentation_offset),
        root_alignment,
    )
}

fn align_measured_point(
    point: ViewPoint,
    scene_origin: ViewPoint,
    presentation_offset: ViewPoint,
    root_alignment: ViewPoint,
) -> ViewPoint {
    let point =
        remove_point_presentation_offset(relative_point(point, scene_origin), presentation_offset);
    ViewPoint::new(point.x + root_alignment.x, point.y + root_alignment.y)
}

fn translate_view_rect(rect: ViewRect, delta: ViewPoint) -> ViewRect {
    ViewRect::new(
        ViewPoint::new(rect.origin.x + delta.x, rect.origin.y + delta.y),
        rect.size,
    )
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum OpenGpuiMeasurementError {
    #[error("native measurement state is busy in another callback")]
    Busy { node: Option<NodeId> },
    #[error("native measurement sink has no active frame")]
    Inactive { node: NodeId },
    #[error("native measurement frame has no measured scene root")]
    MissingSceneRoot,
    #[error("native measurement frame has no measured root for node {node:?}")]
    MissingNodeRoot { node: NodeId },
    #[error("native measurement is missing real ports {ports:?} for node {node:?}")]
    MissingPortMeasurements { node: NodeId, ports: Vec<PortId> },
    #[error("native measurement contains duplicate real ports {ports:?} for node {node:?}")]
    DuplicatePortMeasurements { node: NodeId, ports: Vec<PortId> },
    #[error("native measurement contains a foreign real port for node {node:?}")]
    InvalidMeasuredPortSet { node: NodeId },
    #[error("native measurement references an unknown node instance")]
    UnknownInstance { node: NodeId },
    #[error("native measurement references port {port:?} outside the current node frame")]
    UnknownPort { node: NodeId, port: PortId },
    #[error("native measurement has invalid bounds for port {port:?} on node {node:?}")]
    InvalidPortBounds { node: NodeId, port: PortId },
    #[error("native content measurement has invalid bounds for node {node:?}")]
    InvalidContentBounds { node: NodeId },
    #[error("native measurement has invalid port coverage for node {node:?}: {source}")]
    InvalidPortCoverage {
        node: NodeId,
        #[source]
        source: NodeLayoutBuilderError,
    },
}

impl OpenGpuiMeasurementError {
    pub(crate) fn node(&self) -> Option<NodeId> {
        match self {
            Self::Busy { node } => *node,
            Self::Inactive { node }
            | Self::MissingNodeRoot { node }
            | Self::MissingPortMeasurements { node, .. }
            | Self::DuplicatePortMeasurements { node, .. }
            | Self::InvalidMeasuredPortSet { node }
            | Self::UnknownInstance { node }
            | Self::UnknownPort { node, .. }
            | Self::InvalidPortBounds { node, .. }
            | Self::InvalidContentBounds { node }
            | Self::InvalidPortCoverage { node, .. } => Some(*node),
            Self::MissingSceneRoot => None,
        }
    }
}

pub(crate) fn projected_view_rect(
    bounds: CanvasRect,
    transform: ViewportTransform,
) -> Option<ViewRect> {
    let origin = transform.screen_point_for_canvas(bounds.origin)?;
    let projected = ViewRect::new(
        ViewPoint::new(origin.x, origin.y),
        ViewSize::new(
            bounds.size.width * transform.zoom,
            bounds.size.height * transform.zoom,
        ),
    );
    projected.is_positive_finite().then_some(projected)
}

fn view_rect(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect::new(
        ViewPoint::new(bounds.origin.x.as_f32(), bounds.origin.y.as_f32()),
        ViewSize::new(bounds.size.width.as_f32(), bounds.size.height.as_f32()),
    )
}

#[cfg(test)]
fn minimum_hit_bounds(bounds: ViewRect, diameter: f32) -> ViewRect {
    if !diameter.is_finite() || diameter <= 0.0 {
        return bounds;
    }
    let width = bounds.size.width.max(diameter);
    let height = bounds.size.height.max(diameter);
    ViewRect::new(
        ViewPoint::new(
            bounds.origin.x - (width - bounds.size.width) * 0.5,
            bounds.origin.y - (height - bounds.size.height) * 0.5,
        ),
        ViewSize::new(width, height),
    )
}

fn resolve_route_anchor(
    bounds: ViewRect,
    outward: HandlePosition,
    anchor: OpenGpuiPortAnchor,
) -> Option<ViewPoint> {
    if !bounds.is_positive_finite() {
        return None;
    }
    let center = ViewPoint::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    );
    match anchor {
        OpenGpuiPortAnchor::Center => center.is_finite().then_some(center),
        OpenGpuiPortAnchor::Boundary => side_anchor(bounds, outward),
        OpenGpuiPortAnchor::Offset(offset) if offset.is_finite() => {
            let point = ViewPoint::new(center.x + offset.x, center.y + offset.y);
            point.is_finite().then_some(point)
        }
        OpenGpuiPortAnchor::Offset(_) => None,
    }
}

fn relative_rect(bounds: ViewRect, origin: ViewPoint) -> ViewRect {
    ViewRect::new(relative_point(bounds.origin, origin), bounds.size)
}

fn relative_point(point: ViewPoint, origin: ViewPoint) -> ViewPoint {
    ViewPoint::new(point.x - origin.x, point.y - origin.y)
}

#[cfg(test)]
mod tests {
    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, NodeKindKey, Port,
        PortCapacity, PortKey, PortKind,
    };
    use jellyflow_runtime::{
        commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
        commands::TopologyPort, graph::GraphRuntime, view::FrameRequest, view::RendererStamp,
        view::ViewSessionOptions,
    };
    use open_gpui::accesskit::{Action, Node};

    use super::*;

    #[test]
    fn default_port_geometry_uses_center_anchor_and_minimum_hit_target() {
        let visual = ViewRect::new(ViewPoint::new(10.0, 20.0), ViewSize::new(10.0, 12.0));

        let hit = minimum_hit_bounds(visual, OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER);
        let anchor =
            resolve_route_anchor(visual, HandlePosition::Right, OpenGpuiPortAnchor::default())
                .expect("valid center anchor");

        assert_eq!(visual.origin, ViewPoint::new(10.0, 20.0));
        assert_eq!(visual.size, ViewSize::new(10.0, 12.0));
        assert_eq!(hit.origin, ViewPoint::new(4.0, 15.0));
        assert_eq!(hit.size, ViewSize::new(22.0, 22.0));
        assert_eq!(anchor, ViewPoint::new(15.0, 26.0));

        let pixel_hit = minimum_hit_pixel_bounds(
            Bounds {
                origin: point(px(10.0), px(20.0)),
                size: size(px(10.0), px(12.0)),
            },
            OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
        );
        assert_eq!(pixel_hit.origin, point(px(4.0), px(15.0)));
        assert_eq!(pixel_hit.size, size(px(22.0), px(22.0)));
    }

    #[test]
    fn native_content_mask_clips_interaction_without_moving_visual_geometry() {
        let visual = pixel_bounds(90.0, 40.0, 12.0, 12.0);
        let hit = minimum_hit_pixel_bounds(visual, 22.0);
        let content = pixel_bounds(0.0, 0.0, 96.0, 80.0);

        let interaction = clipped_interaction_pixel_bounds(visual, hit, content)
            .expect("partially visible port remains interactive");

        assert_eq!(visual, pixel_bounds(90.0, 40.0, 12.0, 12.0));
        assert_eq!(interaction, pixel_bounds(85.0, 35.0, 11.0, 22.0));
    }

    #[test]
    fn fully_clipped_visual_port_has_no_effective_interaction_bounds() {
        let visual = pixel_bounds(98.0, 40.0, 12.0, 12.0);
        let hit = minimum_hit_pixel_bounds(visual, 22.0);
        let content = pixel_bounds(0.0, 0.0, 96.0, 80.0);

        assert_eq!(clipped_interaction_pixel_bounds(visual, hit, content), None);
    }

    #[test]
    fn boundary_anchor_uses_outward_side_without_changing_visual_bounds() {
        let visual = ViewRect::new(ViewPoint::new(10.0, 20.0), ViewSize::new(10.0, 12.0));

        let anchor =
            resolve_route_anchor(visual, HandlePosition::Right, OpenGpuiPortAnchor::Boundary)
                .expect("valid boundary anchor");

        assert_eq!(anchor, ViewPoint::new(20.0, 26.0));
        assert_eq!(visual.origin, ViewPoint::new(10.0, 20.0));
        assert_eq!(visual.size, ViewSize::new(10.0, 12.0));
    }

    #[test]
    fn offset_anchor_is_relative_to_visual_center() {
        let visual = ViewRect::new(ViewPoint::new(10.0, 20.0), ViewSize::new(10.0, 12.0));

        let anchor = resolve_route_anchor(
            visual,
            HandlePosition::Left,
            OpenGpuiPortAnchor::Offset(ViewPoint::new(3.0, -2.0)),
        )
        .expect("valid offset anchor");

        assert_eq!(anchor, ViewPoint::new(18.0, 24.0));
    }

    #[test]
    fn visual_hit_anchor_and_outward_geometry_are_independent() {
        let visual = ViewRect::new(ViewPoint::new(4.0, 6.0), ViewSize::new(30.0, 8.0));
        let compact_hit = minimum_hit_bounds(visual, 16.0);
        let large_hit = minimum_hit_bounds(visual, 40.0);
        let left_center =
            resolve_route_anchor(visual, HandlePosition::Left, OpenGpuiPortAnchor::Center)
                .expect("valid left center anchor");
        let right_center =
            resolve_route_anchor(visual, HandlePosition::Right, OpenGpuiPortAnchor::Center)
                .expect("valid right center anchor");

        assert_eq!(
            visual,
            ViewRect::new(ViewPoint::new(4.0, 6.0), ViewSize::new(30.0, 8.0))
        );
        assert_eq!(compact_hit.origin, ViewPoint::new(4.0, 2.0));
        assert_eq!(compact_hit.size, ViewSize::new(30.0, 16.0));
        assert_eq!(large_hit.origin, ViewPoint::new(-1.0, -10.0));
        assert_eq!(large_hit.size, ViewSize::new(40.0, 40.0));
        assert_eq!(left_center, ViewPoint::new(19.0, 10.0));
        assert_eq!(right_center, left_center);
    }

    #[test]
    fn minimum_hit_target_stays_22_screen_pixels_at_every_zoom() {
        let visual = ViewRect::new(ViewPoint::new(30.0, 40.0), ViewSize::new(8.0, 10.0));
        let hit = minimum_hit_bounds(visual, OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER);

        for zoom in [0.5, 1.0, 2.0] {
            let transform = ViewportTransform::new(CanvasPoint::default(), zoom)
                .expect("valid viewport transform");
            let canvas_hit = hit
                .to_canvas_rect(transform)
                .expect("valid canvas hit bounds");
            let projected =
                projected_view_rect(canvas_hit, transform).expect("valid projected hit bounds");

            assert_eq!(projected.size, ViewSize::new(22.0, 22.0));
        }
    }

    #[test]
    fn wrapper_identity_allows_repeat_prepaint_and_rejects_duplicate_semantics() {
        let node_id = NodeId::from_u128(0x21);
        let port_id = PortId::from_u128(0x22);
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x23));
        graph.insert_node(
            node_id,
            jellyflow_core::Node {
                kind: NodeKindKey::new("test.wrapper-identity"),
                kind_version: 1,
                pos: CanvasPoint::default(),
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
                ports: vec![port_id],
                data: serde_json::Value::Null,
            },
        );
        graph.insert_port(port_id, test_port(node_id, "output", PortDirection::Out));
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph.build().expect("valid wrapper identity graph"))
            .expect("wrapper identity runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("wrapper identity view");
        let request = || {
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -100.0,
                        y: -100.0,
                    },
                    size: CanvasSize {
                        width: 400.0,
                        height: 300.0,
                    },
                },
                RendererStamp::new("test.wrapper-identity", 1),
            )
        };

        let repeated = view.frame(request()).expect("repeat-prepaint frame");
        let instance = repeated.nodes()[0].instance().clone();
        let port_instance = repeated.nodes()[0].ports()[0].instance();
        let measurements = OpenGpuiPendingMeasurements::new(
            repeated.nodes(),
            repeated.transform(),
            BTreeMap::new(),
            0,
        );
        measurements
            .activate(&instance, OpenGpuiGraphPresentationSource::Detail, 0)
            .expect("activate repeated measurement");
        measurements.record_scene(pixel_bounds(0.0, 0.0, 400.0, 300.0));
        measurements
            .record_root(
                &instance,
                OpenGpuiGraphPresentationSource::Detail,
                pixel_bounds(10.0, 20.0, 180.0, 100.0),
            )
            .expect("record repeated root");
        measurements
            .record_port(
                &instance,
                OpenGpuiGraphPresentationSource::Detail,
                OpenGpuiPortWrapperId(7),
                raw_port_measurement(port_instance, 40.0),
            )
            .expect("record initial prepaint");
        measurements
            .record_port(
                &instance,
                OpenGpuiGraphPresentationSource::Detail,
                OpenGpuiPortWrapperId(7),
                raw_port_measurement(port_instance, 56.0),
            )
            .expect("record final prepaint");
        let (layouts, errors) = measurements.seal_layouts().into_parts();
        assert!(errors.is_empty());
        assert_eq!(layouts.len(), 1);
        let collector = repeated.collector();
        collector
            .layout(layouts.into_iter().next().unwrap())
            .unwrap();
        view.submit_feedback(collector.finish().unwrap()).unwrap();
        let geometry = view.geometry_snapshot().unwrap();
        assert_eq!(
            geometry.node(node_id).unwrap().bounds().origin,
            CanvasPoint::default()
        );
        assert_eq!(
            geometry.port(port_id).unwrap().visual_bounds().origin,
            CanvasPoint { x: 166.0, y: 36.0 }
        );

        let duplicate = view.frame(request()).expect("duplicate-wrapper frame");
        let instance = duplicate.nodes()[0].instance().clone();
        let port_instance = duplicate.nodes()[0].ports()[0].instance();
        let measurements = OpenGpuiPendingMeasurements::new(
            duplicate.nodes(),
            duplicate.transform(),
            BTreeMap::new(),
            0,
        );
        measurements
            .activate(&instance, OpenGpuiGraphPresentationSource::Detail, 0)
            .unwrap();
        measurements.record_scene(pixel_bounds(0.0, 0.0, 400.0, 300.0));
        measurements
            .record_root(
                &instance,
                OpenGpuiGraphPresentationSource::Detail,
                pixel_bounds(10.0, 20.0, 180.0, 100.0),
            )
            .unwrap();
        for (wrapper, y) in [
            (OpenGpuiPortWrapperId(8), 56.0),
            (OpenGpuiPortWrapperId(9), 72.0),
        ] {
            measurements
                .record_port(
                    &instance,
                    OpenGpuiGraphPresentationSource::Detail,
                    wrapper,
                    raw_port_measurement(port_instance, y),
                )
                .unwrap();
        }
        let (layouts, errors) = measurements.seal_layouts().into_parts();
        assert!(layouts.is_empty());
        assert_eq!(
            errors,
            vec![OpenGpuiMeasurementError::DuplicatePortMeasurements {
                node: node_id,
                ports: vec![port_id],
            }]
        );
    }

    #[test]
    fn same_id_port_replacement_rejects_old_measurement_identity_and_admits_new_identity() {
        let node_id = NodeId::from_u128(0x31);
        let port_id = PortId::from_u128(0x32);
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x33));
        graph.insert_node(
            node_id,
            jellyflow_core::Node {
                kind: NodeKindKey::new("test.port-aba"),
                kind_version: 1,
                pos: CanvasPoint::default(),
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
                ports: vec![port_id],
                data: serde_json::Value::Null,
            },
        );
        let initial_port = test_port(node_id, "value", PortDirection::Out);
        graph.insert_port(port_id, initial_port.clone());
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph.build().expect("valid port ABA graph"))
            .expect("port ABA runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("port ABA view");
        let request = || {
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -100.0,
                        y: -100.0,
                    },
                    size: CanvasSize {
                        width: 400.0,
                        height: 300.0,
                    },
                },
                RendererStamp::new("test.port-aba", 1),
            )
        };

        let initial_frame = view.frame(request()).expect("initial port ABA frame");
        let initial_node_instance = initial_frame.nodes()[0].instance().clone();
        let initial_port_instance = initial_frame.nodes()[0].ports()[0].instance();
        let initial_pending = OpenGpuiPendingMeasurements::new(
            initial_frame.nodes(),
            initial_frame.transform(),
            BTreeMap::new(),
            0,
        );
        initial_pending
            .activate(
                &initial_node_instance,
                OpenGpuiGraphPresentationSource::Detail,
                0,
            )
            .unwrap();
        assert!(initial_pending.admits_port(
            &initial_node_instance,
            initial_port_instance,
            OpenGpuiGraphPresentationSource::Detail,
        ));

        let mut replacement = initial_port;
        replacement.key = PortKey::new("replacement-value");
        runtime
            .execute(GraphCommand::topology(
                initial_frame.graph_version(),
                TopologyEdit::new().replace_node_ports(
                    node_id,
                    [TopologyPort::new(port_id, replacement)],
                    IncidentEdgePolicy::Reject,
                ),
            ))
            .unwrap()
            .applied()
            .expect("same-ID port replacement applies");

        let replacement_frame = view.frame(request()).expect("replacement port ABA frame");
        let replacement_node_instance = replacement_frame.nodes()[0].instance().clone();
        let replacement_port_instance = replacement_frame.nodes()[0].ports()[0].instance();
        assert_eq!(replacement_node_instance, initial_node_instance);
        assert_ne!(replacement_port_instance, initial_port_instance);
        let replacement_pending = OpenGpuiPendingMeasurements::new(
            replacement_frame.nodes(),
            replacement_frame.transform(),
            BTreeMap::new(),
            0,
        );
        replacement_pending
            .activate(
                &replacement_node_instance,
                OpenGpuiGraphPresentationSource::Detail,
                0,
            )
            .unwrap();
        assert!(!replacement_pending.admits_port(
            &replacement_node_instance,
            initial_port_instance,
            OpenGpuiGraphPresentationSource::Detail,
        ));
        assert!(replacement_pending.admits_port(
            &replacement_node_instance,
            replacement_port_instance,
            OpenGpuiGraphPresentationSource::Detail,
        ));
        assert_eq!(
            replacement_pending.record_port(
                &replacement_node_instance,
                OpenGpuiGraphPresentationSource::Detail,
                OpenGpuiPortWrapperId(1),
                raw_port_measurement(initial_port_instance, 40.0),
            ),
            Err(OpenGpuiMeasurementError::UnknownPort {
                node: node_id,
                port: port_id,
            }),
        );

        let clean_pending = OpenGpuiPendingMeasurements::new(
            replacement_frame.nodes(),
            replacement_frame.transform(),
            BTreeMap::new(),
            0,
        );
        clean_pending
            .activate(
                &replacement_node_instance,
                OpenGpuiGraphPresentationSource::Detail,
                0,
            )
            .unwrap();
        clean_pending
            .record_port(
                &replacement_node_instance,
                OpenGpuiGraphPresentationSource::Detail,
                OpenGpuiPortWrapperId(2),
                raw_port_measurement(replacement_port_instance, 56.0),
            )
            .expect("replacement identity publishes measurement");
    }

    #[test]
    fn static_and_interactive_ports_expose_distinct_accesskit_contracts() {
        let mut static_node = Node::new(OPEN_GPUI_STATIC_PORT_ROLE);
        write_static_port_accessibility(&mut static_node, "Static value output");
        assert_eq!(static_node.role(), Role::Image);
        assert_eq!(static_node.label(), Some("Static value output"));
        assert!(!static_node.supports_action(Action::Click));
        assert!(!static_node.supports_action(Action::Focus));

        let enabled = interactive_accesskit_node(true);
        assert_eq!(enabled.role(), Role::Button);
        assert_eq!(enabled.label(), Some("Interactive value output"));
        assert!(!enabled.is_disabled());
        assert!(enabled.supports_action(Action::Click));
        assert!(enabled.supports_action(Action::Focus));

        let disabled = interactive_accesskit_node(false);
        assert_eq!(disabled.role(), Role::Button);
        assert_eq!(disabled.label(), Some("Interactive value output"));
        assert!(disabled.is_disabled());
        assert!(!disabled.supports_action(Action::Click));
        assert!(disabled.supports_action(Action::Focus));
    }

    #[test]
    fn missing_node_root_omits_layout_and_retains_previous_port_geometry() {
        let node_id = NodeId::from_u128(0x10);
        let input = PortId::from_u128(0x11);
        let output = PortId::from_u128(0x12);
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x13));
        graph.insert_node(
            node_id,
            jellyflow_core::Node {
                kind: NodeKindKey::new("test.root-fallback"),
                kind_version: 1,
                pos: CanvasPoint { x: 20.0, y: 30.0 },
                origin: None,
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(CanvasSize {
                    width: 220.0,
                    height: 120.0,
                }),
                hidden: false,
                collapsed: false,
                ports: vec![input, output],
                data: serde_json::Value::Null,
            },
        );
        graph.insert_port(input, test_port(node_id, "input", PortDirection::In));
        graph.insert_port(output, test_port(node_id, "output", PortDirection::Out));
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph.build().expect("valid root fallback graph"))
            .expect("root fallback runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("root fallback view");
        view.set_viewport(
            ViewportTransform::new(CanvasPoint { x: 7.0, y: -3.0 }, 2.0)
                .expect("valid root fallback viewport transform"),
        )
        .expect("valid root fallback viewport");
        let request = || {
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -100.0,
                        y: -100.0,
                    },
                    size: CanvasSize {
                        width: 1_000.0,
                        height: 600.0,
                    },
                },
                RendererStamp::new("test.root-fallback", 1),
            )
        };

        let initial = view.frame(request()).expect("initial root fallback frame");
        let frame_node = initial
            .nodes()
            .first()
            .expect("admitted root fallback node");
        let root = ViewRect::new(ViewPoint::new(54.0, 54.0), ViewSize::new(440.0, 240.0));
        let mut initial_layout = frame_node.begin_layout(root);
        for (index, port) in frame_node.ports().iter().enumerate() {
            let x = if port.id() == input { 54.0 } else { 480.0 };
            let visual = ViewRect::new(
                ViewPoint::new(x, 80.0 + index as f32 * 24.0),
                ViewSize::new(14.0, 14.0),
            );
            initial_layout
                .port_rendered(
                    port.layout_target(),
                    visual,
                    visual,
                    ViewPoint::new(x, visual.origin.y + 7.0),
                    if port.id() == input {
                        HandlePosition::Left
                    } else {
                        HandlePosition::Right
                    },
                )
                .expect("cover initial frame port");
        }
        let collector = initial.collector();
        collector
            .layout(
                initial_layout
                    .finish()
                    .expect("complete initial root fallback layout"),
            )
            .expect("collect initial root fallback layout");
        view.submit_feedback(collector.finish().expect("initial feedback"))
            .expect("publish initial port geometry");
        let initial_geometry = view.geometry_snapshot().expect("initial geometry");
        assert!(initial_geometry.port(input).is_some());
        assert!(initial_geometry.port(output).is_some());

        let fallback = view.frame(request()).expect("fallback frame");
        let measurements = OpenGpuiPendingMeasurements::new(
            fallback.nodes(),
            fallback.transform(),
            BTreeMap::new(),
            0,
        );
        measurements.record_scene(Bounds {
            origin: point(px(100.0), px(200.0)),
            size: size(px(800.0), px(600.0)),
        });
        let (layouts, errors) = measurements.seal_layouts().into_parts();
        assert!(layouts.is_empty());
        assert_eq!(
            errors,
            vec![OpenGpuiMeasurementError::MissingNodeRoot { node: node_id }]
        );

        let collector = fallback.collector();
        view.submit_feedback(collector.finish().expect("fallback feedback"))
            .expect("submit feedback without transient layout");
        let geometry = view.geometry_snapshot().expect("fallback geometry");
        for port in [input, output] {
            assert_eq!(geometry.port(port), initial_geometry.port(port));
        }
    }

    fn interactive_accesskit_node(enabled: bool) -> Node {
        let element = interactive_port_semantic_element(
            "test-semantic-port".into(),
            "Interactive value output".to_owned(),
            enabled,
            None,
            div(),
            |_, _, _| {},
            |_, _, _| {},
        );
        assert_eq!(element.a11y_role(), Some(OPEN_GPUI_INTERACTIVE_PORT_ROLE));
        let mut node = Node::new(OPEN_GPUI_INTERACTIVE_PORT_ROLE);
        element.write_a11y_info(&mut node);
        node
    }

    fn pixel_bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(width), px(height)),
        }
    }

    fn raw_port_measurement(port_instance: PortInstanceKey, y: f32) -> RawPortMeasurement {
        let visual_bounds = ViewRect::new(ViewPoint::new(176.0, y), ViewSize::new(14.0, 14.0));
        RawPortMeasurement {
            port_instance,
            visual_bounds,
            hit_bounds: minimum_hit_bounds(visual_bounds, 22.0),
            interaction_bounds: Some(minimum_hit_bounds(visual_bounds, 22.0)),
            route_anchor: ViewPoint::new(183.0, y + 7.0),
            outward: HandlePosition::Right,
        }
    }

    fn test_port(node: NodeId, key: &str, direction: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir: direction,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }
}
