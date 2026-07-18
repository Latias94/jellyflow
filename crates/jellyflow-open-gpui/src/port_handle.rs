use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use jellyflow::core::{CanvasPoint, CanvasRect, NodeId, PortDirection, PortId};
use jellyflow::runtime::runtime::geometry::HandlePosition;
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{
    FrameNode, NodeInstanceKey, NodeLayout, NodeLayoutBuilderError, ViewPoint, ViewRect, ViewSize,
    side_anchor,
};
use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Div, Element, ElementId, EntityId, FocusHandle,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement, IntoElement,
    KeyUpEvent, LayoutId, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Pixels, Role,
    Stateful, StatefulInteractiveElement, Window, div, point, px, size,
};

use crate::accessibility::{
    OPEN_GPUI_INTERACTIVE_PORT_ROLE, OPEN_GPUI_STATIC_PORT_ROLE, open_gpui_port_element_id,
    open_gpui_port_measurement_element_id,
};
use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::OpenGpuiMountCapability;
use crate::interaction::{
    OpenGpuiInteractionCoordinator, OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragOwner, OpenGpuiPortDragPhase, OpenGpuiPortEvent,
    OpenGpuiPortListener, port_drag_owner,
};
use crate::presentation::OpenGpuiPortInteractionState;
use crate::theme::OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER;

pub(crate) type OpenGpuiHostPortListener =
    Rc<dyn Fn(&OpenGpuiPortEvent, &mut Window, &mut App) + 'static>;

/// Edge-routing side associated with one native port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiPortSide {
    Top,
    Right,
    Bottom,
    Left,
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
    instance: NodeInstanceKey,
    port: PortId,
    side: OpenGpuiPortSide,
    accessible_name: String,
    interaction_enabled: bool,
    presentation: OpenGpuiPortInteractionState,
    focus_handle: Option<FocusHandle>,
    anchor: OpenGpuiPortAnchor,
    minimum_hit_diameter: f32,
    sink: OpenGpuiMeasurementSink,
}

impl std::fmt::Debug for OpenGpuiPortHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiPortHandle")
            .field("instance", &self.instance)
            .field("port", &self.port)
            .field("side", &self.side)
            .field("accessible_name", &self.accessible_name)
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
        port: PortId,
        direction: PortDirection,
        name: String,
        sink: OpenGpuiMeasurementSink,
    ) -> Self {
        let direction_name = match direction {
            PortDirection::In => "input",
            PortDirection::Out => "output",
        };
        Self {
            instance,
            port,
            side: OpenGpuiPortSide::for_direction(direction),
            accessible_name: format!("{name} {direction_name} port"),
            interaction_enabled: true,
            presentation: OpenGpuiPortInteractionState::idle(),
            focus_handle: None,
            anchor: OpenGpuiPortAnchor::default(),
            minimum_hit_diameter: OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
            sink,
        }
    }

    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    #[must_use]
    pub fn port(&self) -> PortId {
        self.port
    }

    #[must_use]
    pub fn side(&self) -> OpenGpuiPortSide {
        self.side
    }

    /// Returns the name exposed to assistive technology for this real graph port.
    #[must_use]
    pub fn accessible_name(&self) -> &str {
        &self.accessible_name
    }

    /// Replaces the semantic port-key fallback with an application-facing accessible name.
    #[must_use]
    pub fn with_accessible_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        if !name.trim().is_empty() {
            self.accessible_name = name;
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

    /// Wraps an ordinary Open GPUI element and reports its post-layout bounds as this real port.
    pub fn measure(&self, child: impl IntoElement) -> impl IntoElement {
        OpenGpuiPortMeasurementElement {
            id: open_gpui_port_element_id(self.sink.host, &self.instance, self.port),
            child: Some(child.into_any_element()),
            port: self.port,
            side: self.side,
            anchor: self.anchor,
            minimum_hit_diameter: self.minimum_hit_diameter,
            accessible_name: self.semantic_accessible_name(),
            sink: self.sink.clone(),
        }
    }

    /// Wraps a native port marker with measurement, pointer targeting, capture, and typed events.
    ///
    /// The returned element remains a thin graph-semantic boundary. Its child is still an ordinary
    /// Open GPUI element, while the callback receives start, move, release, and cancellation facts.
    pub fn interactive(
        &self,
        child: impl IntoElement,
        listener: impl Fn(OpenGpuiPortEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let interaction_enabled =
            self.interaction_enabled && !self.presentation.disabled() && self.sink.mount.is_live();
        let semantic_id = open_gpui_port_element_id(self.sink.host, &self.instance, self.port);
        let measurement_id =
            open_gpui_port_measurement_element_id(self.sink.host, &self.instance, self.port);
        let owner = port_drag_owner(self.instance.clone(), self.port);
        let product_listener: OpenGpuiPortListener = Rc::new(listener);
        let host_listener = self.sink.host_port_listener.clone();
        let mount = self.sink.mount.clone();
        let listener: OpenGpuiPortListener = Rc::new(move |event, window, cx| {
            if !mount.is_live() && !is_retirement_cancellation(&event) {
                return;
            }
            host_listener(&event, window, cx);
            product_listener(event, window, cx);
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
            owner,
            side: self.side,
            anchor: self.anchor,
            interaction_enabled,
            minimum_hit_diameter: self.minimum_hit_diameter,
            sink: self.sink.clone(),
            activity: self.sink.activity.clone(),
            coordinator: self.sink.interactions.clone(),
            listener,
        }
    }

    fn semantic_accessible_name(&self) -> String {
        format!(
            "{}, {}",
            self.accessible_name,
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

struct OpenGpuiPortMeasurementElement {
    id: ElementId,
    child: Option<AnyElement>,
    port: PortId,
    side: OpenGpuiPortSide,
    anchor: OpenGpuiPortAnchor,
    minimum_hit_diameter: f32,
    accessible_name: String,
    sink: OpenGpuiMeasurementSink,
}

impl Element for OpenGpuiPortMeasurementElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

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
    ) {
        child.prepaint(window, cx);
        let _ = self.sink.record_port(
            self.port,
            bounds,
            self.side,
            self.anchor,
            self.minimum_hit_diameter,
        );
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
    owner: OpenGpuiPortDragOwner,
    side: OpenGpuiPortSide,
    anchor: OpenGpuiPortAnchor,
    interaction_enabled: bool,
    minimum_hit_diameter: f32,
    sink: OpenGpuiMeasurementSink,
    activity: OpenGpuiNodeActivityRegistry,
    coordinator: OpenGpuiInteractionCoordinator,
    listener: OpenGpuiPortListener,
}

impl Element for OpenGpuiPortInteractionElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = Hitbox;

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
        let _ = self.sink.record_port(
            self.owner.port(),
            bounds,
            self.side,
            self.anchor,
            self.minimum_hit_diameter,
        );
        let hitbox = window.insert_hitbox(
            minimum_hit_pixel_bounds(bounds, self.minimum_hit_diameter),
            HitboxBehavior::BlockMouse,
        );
        if self.sink.mount.is_live() {
            if self.interaction_enabled {
                self.coordinator
                    .rebind_port(&self.owner, &self.sink.mount, hitbox.id, window);
            } else {
                self.coordinator
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
        let owner = self.owner.clone();
        let listener = self.listener.clone();
        let activity = self.activity.clone();
        let mount = self.sink.mount.clone();
        let coordinator = self.coordinator.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
                && coordinator.begin_port_drag(
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

#[derive(Clone)]
pub(crate) struct OpenGpuiMeasurementSink {
    host: EntityId,
    instance: NodeInstanceKey,
    pending: Rc<RefCell<Option<Weak<OpenGpuiPendingMeasurements>>>>,
    errors: Rc<RefCell<Vec<OpenGpuiMeasurementError>>>,
    error_queue_busy: Rc<Cell<bool>>,
    activity: OpenGpuiNodeActivityRegistry,
    interactions: OpenGpuiInteractionCoordinator,
    host_port_listener: OpenGpuiHostPortListener,
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
        interactions: OpenGpuiInteractionCoordinator,
        host_port_listener: OpenGpuiHostPortListener,
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
            mount,
        }
    }

    pub(crate) fn activate(
        &self,
        pending: &Rc<OpenGpuiPendingMeasurements>,
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
        *current = Some(Rc::downgrade(pending));
        Ok(())
    }

    fn record_port(
        &self,
        port: PortId,
        bounds: Bounds<Pixels>,
        side: OpenGpuiPortSide,
        anchor: OpenGpuiPortAnchor,
        minimum_hit_diameter: f32,
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
                    port,
                },
            )?;
            pending.record_port(
                &self.instance,
                RawPortMeasurement {
                    port,
                    visual_bounds,
                    hit_bounds: minimum_hit_bounds(visual_bounds, minimum_hit_diameter),
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
    expected_ports: BTreeSet<PortId>,
    root: Option<ViewRect>,
    ports: BTreeMap<PortId, RawPortMeasurement>,
}

struct RawPortMeasurement {
    port: PortId,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    route_anchor: ViewPoint,
    outward: HandlePosition,
}

pub(crate) struct OpenGpuiPendingMeasurements {
    order: Vec<NodeInstanceKey>,
    nodes: RefCell<BTreeMap<NodeInstanceKey, PendingNodeMeasurement>>,
    transform: ViewportTransform,
    presentation_offsets: BTreeMap<NodeId, CanvasPoint>,
    scene_origin: Cell<Option<ViewPoint>>,
    errors: RefCell<Vec<OpenGpuiMeasurementError>>,
    error_queue_busy: Cell<bool>,
}

impl OpenGpuiPendingMeasurements {
    pub(crate) fn new(
        frame: &[FrameNode],
        transform: ViewportTransform,
        presentation_offsets: BTreeMap<NodeId, CanvasPoint>,
    ) -> Self {
        let order = frame.iter().map(|node| node.instance().clone()).collect();
        let nodes = frame
            .iter()
            .map(|node| {
                (
                    node.instance().clone(),
                    PendingNodeMeasurement {
                        frame_node: node.clone(),
                        expected_ports: node.ports().iter().map(|port| port.id()).collect(),
                        root: None,
                        ports: BTreeMap::new(),
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
        if !node.expected_ports.contains(&measurement.port) {
            return Err(OpenGpuiMeasurementError::UnknownPort {
                node: instance.node(),
                port: measurement.port,
            });
        }
        node.ports.insert(measurement.port, measurement);
        Ok(())
    }

    pub(crate) fn layouts(&self) -> Result<Vec<NodeLayout>, OpenGpuiMeasurementError> {
        let scene_origin = self
            .scene_origin
            .get()
            .ok_or(OpenGpuiMeasurementError::MissingSceneRoot)?;
        let nodes = self
            .nodes
            .try_borrow()
            .map_err(|_| OpenGpuiMeasurementError::Busy { node: None })?;
        let mut layouts = Vec::with_capacity(self.order.len());
        for instance in &self.order {
            let node = nodes
                .get(instance)
                .ok_or(OpenGpuiMeasurementError::UnknownInstance {
                    node: instance.node(),
                })?;
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
            let measured_root = node.root.map(|root| {
                remove_presentation_offset(relative_rect(root, scene_origin), presentation_offset)
            });
            let root = match measured_root {
                Some(root) => root,
                None => projected_view_rect(node.frame_node.bounds(), self.transform).ok_or(
                    OpenGpuiMeasurementError::UnprojectableRoot {
                        node: instance.node(),
                    },
                )?,
            };
            let mut layout = node.frame_node.begin_layout(root);
            for port in node.frame_node.ports() {
                let result = if let Some(measurement) =
                    measured_root.and_then(|_| node.ports.get(&port.id()))
                {
                    layout.port_rendered(
                        port.layout_target(),
                        remove_presentation_offset(
                            relative_rect(measurement.visual_bounds, scene_origin),
                            presentation_offset,
                        ),
                        remove_presentation_offset(
                            relative_rect(measurement.hit_bounds, scene_origin),
                            presentation_offset,
                        ),
                        remove_point_presentation_offset(
                            relative_point(measurement.route_anchor, scene_origin),
                            presentation_offset,
                        ),
                        measurement.outward,
                    )
                } else {
                    layout.port_unavailable(port.layout_target())
                };
                result.map_err(|source| OpenGpuiMeasurementError::InvalidPortCoverage {
                    node: instance.node(),
                    source,
                })?;
            }
            layouts.push(layout.finish().map_err(|source| {
                OpenGpuiMeasurementError::InvalidPortCoverage {
                    node: instance.node(),
                    source,
                }
            })?);
        }
        Ok(layouts)
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

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum OpenGpuiMeasurementError {
    #[error("native measurement state is busy in another callback")]
    Busy { node: Option<NodeId> },
    #[error("native measurement sink has no active frame")]
    Inactive { node: NodeId },
    #[error("native measurement frame has no measured scene root")]
    MissingSceneRoot,
    #[error("native measurement references an unknown node instance")]
    UnknownInstance { node: NodeId },
    #[error("native measurement references port {port:?} outside the current node frame")]
    UnknownPort { node: NodeId, port: PortId },
    #[error("native measurement has invalid bounds for port {port:?} on node {node:?}")]
    InvalidPortBounds { node: NodeId, port: PortId },
    #[error("native measurement cannot project fallback bounds for node {node:?}")]
    UnprojectableRoot { node: NodeId },
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
            | Self::UnknownInstance { node }
            | Self::UnknownPort { node, .. }
            | Self::InvalidPortBounds { node, .. }
            | Self::UnprojectableRoot { node }
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
    use jellyflow::core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, NodeKindKey, Port,
        PortCapacity, PortKey, PortKind,
    };
    use jellyflow::runtime::{
        FrameRequest, GraphRuntime, PortLayoutOutcome, PublishedPortOutcome, RendererStamp,
        ViewSessionOptions,
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
    fn missing_node_root_uses_frame_bounds_and_withdraws_previous_port_hits() {
        let node_id = NodeId::from_u128(0x10);
        let input = PortId::from_u128(0x11);
        let output = PortId::from_u128(0x12);
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x13));
        graph.insert_node(
            node_id,
            jellyflow::core::Node {
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
        view.set_viewport(CanvasPoint { x: 7.0, y: -3.0 }, 2.0)
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
        );
        measurements.record_scene(Bounds {
            origin: point(px(100.0), px(200.0)),
            size: size(px(800.0), px(600.0)),
        });
        let layouts = measurements.layouts().expect("fallback layout");
        assert_eq!(layouts.len(), 1);
        assert_eq!(layouts[0].bounds(), root);
        assert!(
            layouts[0]
                .port_outcomes()
                .iter()
                .all(|outcome| matches!(outcome, PortLayoutOutcome::Unavailable { .. }))
        );

        let collector = fallback.collector();
        collector
            .layout(layouts.into_iter().next().expect("one fallback layout"))
            .expect("collect fallback layout");
        view.submit_feedback(collector.finish().expect("fallback feedback"))
            .expect("publish unavailable port coverage");
        let geometry = view.geometry_snapshot().expect("fallback geometry");
        for port in [input, output] {
            assert!(geometry.port(port).is_none());
            assert!(matches!(
                geometry.port_outcome(port),
                Some(PublishedPortOutcome::Unavailable),
            ));
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
