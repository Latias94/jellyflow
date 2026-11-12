use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::mem;
use std::rc::{Rc, Weak};

use jellyflow_core::PortId;
use jellyflow_runtime::{
    graph::EdgeEndpoint, graph::EdgeInstanceKey, graph::PortInstanceKey, view::NodeInstanceKey,
};
use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, GlobalElementId,
    Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent,
    KeyUpEvent, LayoutId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, PinchEvent, Pixels, Point, PointerCancelEvent, PointerCaptureError,
    PointerCaptureHandle, ScrollEvent, Styled, WeakFocusHandle, Window, div,
};

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::binding::OpenGpuiMountCapability;
use crate::host::native_input::{
    OpenGpuiGraphKeyOwner, OpenGpuiNativeInputHandle, OpenGpuiNativeKeyOwner,
};

/// Why an active native port drag ended without a release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiPortDragCancelReason {
    /// The user pressed Escape while the port owned pointer capture.
    Escape,
    /// The application disabled the real port while it owned pointer capture.
    PortDisabled,
    /// The node instance was hard-retired while the drag was active.
    OwnerRetired,
    /// The host view entered its terminal lifecycle.
    HostClosed,
    /// The view's protected-instance budget rejected the capture lease.
    ProtectedCapacityExceeded,
    /// The activity registry or interaction generation was unavailable.
    InteractionUnavailable,
    /// Another native component replaced this drag's pointer capture.
    PointerCaptureLost,
}

/// Graph-semantic lifecycle phase emitted by
/// [`crate::authoring::OpenGpuiPortHandle::interactive`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiPortDragPhase {
    Start,
    Move,
    Release,
    Cancel(OpenGpuiPortDragCancelReason),
}

/// Stable owner of one native port drag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenGpuiPortDragOwner {
    instance: NodeInstanceKey,
    port_instance: PortInstanceKey,
}

impl OpenGpuiPortDragOwner {
    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    #[must_use]
    pub fn port(&self) -> PortId {
        self.port_instance.port()
    }

    /// Returns the exact runtime lifetime owned by this native interaction.
    #[must_use]
    pub fn port_instance(&self) -> PortInstanceKey {
        self.port_instance
    }
}

/// One native pointer event translated into the graph-semantic port-drag lifecycle.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenGpuiPortDragEvent {
    owner: OpenGpuiPortDragOwner,
    native_generation: Option<NativeInteractionGeneration>,
    phase: OpenGpuiPortDragPhase,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeInteractionGeneration(u64);

impl NativeInteractionGeneration {
    #[cfg(test)]
    pub(crate) const fn test_value(self) -> u64 {
        self.0
    }
}

/// Native source that activated a port without starting a pointer drag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiPortActivationSource {
    Keyboard,
    Accessibility,
}

/// One non-pointer activation of a real graph port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenGpuiPortActivationEvent {
    owner: OpenGpuiPortDragOwner,
    source: OpenGpuiPortActivationSource,
}

impl OpenGpuiPortActivationEvent {
    pub(crate) fn new(owner: OpenGpuiPortDragOwner, source: OpenGpuiPortActivationSource) -> Self {
        Self { owner, source }
    }

    #[must_use]
    pub fn owner(&self) -> &OpenGpuiPortDragOwner {
        &self.owner
    }

    #[must_use]
    pub fn source(&self) -> OpenGpuiPortActivationSource {
        self.source
    }
}

/// One owned native interaction emitted by an interactive real graph port.
#[derive(Clone, Debug, PartialEq)]
pub enum OpenGpuiPortEvent {
    Pointer(OpenGpuiPortDragEvent),
    Activation(OpenGpuiPortActivationEvent),
}

/// Native ownership boundary for a popup or overlay rendered outside its node bounds.
///
/// Ordinary controls and scroll regions inside a node need no Jellyflow wrapper. Overlays are
/// different: their hit area may no longer be beneath the node's semantic root, so this boundary
/// occludes graph targets behind it. The retained overlay entity should also hold an
/// [`OpenGpuiActivityKind::Overlay`] lease while it is open.
#[derive(Clone)]
pub struct OpenGpuiOverlayInteractionBoundary {
    interactions: OpenGpuiNativeInteractionHandle,
}

impl OpenGpuiOverlayInteractionBoundary {
    pub(crate) fn new(interactions: OpenGpuiNativeInteractionHandle) -> Self {
        Self { interactions }
    }

    /// Occludes graph hit targets behind a native overlay and stops its unhandled pointer input.
    pub fn wrap(&self, child: impl IntoElement) -> impl IntoElement {
        OpenGpuiOverlayInteractionElement::new(self.interactions.clone(), child)
    }

    /// Occludes graph input and registers the native focus domain owned by this overlay.
    ///
    /// Registering the domain lets the host restore connection-search focus precisely and clear
    /// focus when the overlay is removed during terminal host cleanup. Unrelated native focus is
    /// never changed.
    pub fn wrap_focus_domain(
        &self,
        focus: &FocusHandle,
        child: impl IntoElement,
    ) -> impl IntoElement {
        OpenGpuiOverlayInteractionElement::new_with_focus(
            self.interactions.clone(),
            focus.clone(),
            child,
        )
    }
}

impl OpenGpuiPortDragEvent {
    #[must_use]
    pub fn owner(&self) -> &OpenGpuiPortDragOwner {
        &self.owner
    }

    pub(crate) fn native_generation(&self) -> Option<NativeInteractionGeneration> {
        self.native_generation
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        owner: OpenGpuiPortDragOwner,
        native_generation: u64,
        phase: OpenGpuiPortDragPhase,
        position: Point<Pixels>,
    ) -> Self {
        Self {
            owner,
            native_generation: Some(NativeInteractionGeneration(native_generation)),
            phase,
            position,
            modifiers: Modifiers::default(),
        }
    }

    #[must_use]
    pub fn phase(&self) -> OpenGpuiPortDragPhase {
        self.phase
    }

    #[must_use]
    pub fn position(&self) -> Point<Pixels> {
        self.position
    }

    #[must_use]
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

pub(crate) type OpenGpuiPortListener =
    Rc<dyn Fn(OpenGpuiPortEvent, &mut Window, &mut App) + 'static>;

trait CaptureGeneration {
    fn generation(&self) -> u64;
}

struct CaptureBookkeeping<Active, Pending, Suppression> {
    next_generation: Option<u64>,
    active: Option<Active>,
    pending_cancellations: VecDeque<Pending>,
    drain_scheduled: bool,
    suppression: Suppression,
}

impl<Active, Pending, Suppression> CaptureBookkeeping<Active, Pending, Suppression> {
    fn new(suppression: Suppression) -> Self {
        Self {
            next_generation: Some(1),
            active: None,
            pending_cancellations: VecDeque::new(),
            drain_scheduled: false,
            suppression,
        }
    }

    fn allocate_generation(&mut self) -> Option<u64> {
        let generation = self.next_generation?;
        self.next_generation = generation.checked_add(1);
        Some(generation)
    }

    fn latest_generation(&self) -> u64 {
        self.next_generation
            .and_then(|next| next.checked_sub(1))
            .unwrap_or(u64::MAX)
    }

    fn active(&self) -> Option<&Active> {
        self.active.as_ref()
    }

    fn active_mut(&mut self) -> Option<&mut Active> {
        self.active.as_mut()
    }

    fn activate(&mut self, active: Active) {
        debug_assert!(self.active.is_none());
        self.active = Some(active);
    }

    fn take_active(&mut self) -> Option<Active> {
        self.active.take()
    }

    fn enqueue(&mut self, pending: Pending, suppression: Suppression) {
        self.pending_cancellations.push_back(pending);
        self.suppression = suppression;
    }

    fn prepare_pending_drain(&mut self) -> bool {
        if self.pending_cancellations.is_empty() || self.drain_scheduled {
            return false;
        }
        self.drain_scheduled = true;
        true
    }

    fn take_pending(&mut self) -> VecDeque<Pending> {
        self.drain_scheduled = false;
        mem::take(&mut self.pending_cancellations)
    }

    fn requires_cleanup(&self) -> bool {
        self.active.is_some() || !self.pending_cancellations.is_empty()
    }

    fn is_busy(&self) -> bool {
        self.requires_cleanup()
    }

    fn suppression(&self) -> Suppression
    where
        Suppression: Copy,
    {
        self.suppression
    }

    fn set_suppression(&mut self, suppression: Suppression) {
        self.suppression = suppression;
    }
}

impl<Active, Pending, Suppression> CaptureBookkeeping<Active, Pending, Suppression>
where
    Active: CaptureGeneration,
{
    fn take_active_for_generation(&mut self, generation: u64) -> Option<Active> {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.generation() != generation)
        {
            return None;
        }
        self.active.take()
    }
}

#[derive(Clone)]
pub(crate) struct OpenGpuiNativeInteractionHandle {
    state: Rc<RefCell<NativeInteractionState>>,
}

pub(crate) struct OpenGpuiNativeInteractionAuthority {
    handle: OpenGpuiNativeInteractionHandle,
}

impl std::ops::Deref for OpenGpuiNativeInteractionAuthority {
    type Target = OpenGpuiNativeInteractionHandle;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl OpenGpuiNativeInteractionAuthority {
    pub(crate) fn new() -> Self {
        Self {
            handle: OpenGpuiNativeInteractionHandle::new(),
        }
    }

    pub(crate) fn capability(&self) -> OpenGpuiNativeInteractionHandle {
        self.handle.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeReleaseSuppression {
    button: MouseButton,
    generation: u64,
    handle: PointerCaptureHandle,
}

impl NativeReleaseSuppression {
    fn new(button: MouseButton, generation: u64, handle: PointerCaptureHandle) -> Self {
        Self {
            button,
            generation,
            handle,
        }
    }
}

type NativeCaptureBookkeeping =
    CaptureBookkeeping<NativeCapture, PendingNativeCancellation, Option<NativeReleaseSuppression>>;

struct NativeInteractionState {
    capture: NativeCaptureBookkeeping,
    capture_handle: Option<PointerCaptureHandle>,
    focus_handle: Option<FocusHandle>,
    port_focus: Option<PortFocusOwnership>,
    keyboard_ports: BTreeMap<PortInstanceKey, (OpenGpuiPortDragOwner, OpenGpuiPortListener)>,
    overlay_active: bool,
    overlay_focus_domains: Vec<WeakFocusHandle>,
    space_pan_held: bool,
    terminal: bool,
}

enum NativeCapture {
    Port(ActivePortDrag),
    Scene(ActiveSceneInteraction),
}

enum PendingNativeCancellation {
    Port(Box<PendingPortCancellation>),
    Scene(PendingSceneCancellation),
}

impl PendingNativeCancellation {
    fn generation(&self) -> u64 {
        match self {
            Self::Port(pending) => pending.generation,
            Self::Scene(pending) => pending.generation,
        }
    }

    fn handle(&self) -> PointerCaptureHandle {
        match self {
            Self::Port(pending) => pending.handle,
            Self::Scene(pending) => pending.handle,
        }
    }

    fn button(&self) -> MouseButton {
        match self {
            Self::Port(_) => MouseButton::Left,
            Self::Scene(pending) => pending.button,
        }
    }
}

struct ActivePortDrag {
    generation: u64,
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    listener: OpenGpuiPortListener,
    lease: OpenGpuiActivityLease,
    handle: PointerCaptureHandle,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

struct PendingPortCancellation {
    generation: u64,
    handle: PointerCaptureHandle,
    event: OpenGpuiPortDragEvent,
    listener: OpenGpuiPortListener,
    focus_settlement: Option<PendingPortFocusSettlement>,
}

struct PendingPortFocusSettlement {
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    exit: OpenGpuiPortFocusExit,
}

struct PortFocusOwnership {
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    focus: NativeFocusOwnership,
}

struct NativeFocusOwnership {
    previous_focus: Option<WeakFocusHandle>,
    previous_focus_was_graph_domain: bool,
    interaction_focus: Option<WeakFocusHandle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiPortFocusExit {
    Restore,
    RestoreSearchOverlay,
    OwnerRetired,
    HostClosed,
}

impl CaptureGeneration for ActivePortDrag {
    fn generation(&self) -> u64 {
        self.generation
    }
}

impl CaptureGeneration for NativeCapture {
    fn generation(&self) -> u64 {
        match self {
            Self::Port(active) => active.generation,
            Self::Scene(active) => active.generation,
        }
    }
}

impl OpenGpuiNativeInteractionHandle {
    fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(NativeInteractionState {
                capture: CaptureBookkeeping::new(None),
                capture_handle: None,
                focus_handle: None,
                port_focus: None,
                keyboard_ports: BTreeMap::new(),
                overlay_active: false,
                overlay_focus_domains: Vec::new(),
                space_pan_held: false,
                terminal: false,
            })),
        }
    }

    pub(crate) fn pointer_capture_handle(&self, window: &mut Window) -> PointerCaptureHandle {
        *self
            .state
            .borrow_mut()
            .capture_handle
            .get_or_insert_with(|| window.new_pointer_capture_handle())
    }

    pub(crate) fn set_focus_handle(&self, focus_handle: FocusHandle) -> bool {
        let mut state = self.state.borrow_mut();
        if state.terminal {
            return false;
        }
        state.focus_handle = Some(focus_handle);
        true
    }

    fn begin_graph_paint(&self) {
        let mut state = self.state.borrow_mut();
        if !state.terminal {
            state.overlay_active = false;
            state.overlay_focus_domains.clear();
            state.keyboard_ports.clear();
        }
    }

    pub(crate) fn register_keyboard_port(
        &self,
        owner: OpenGpuiPortDragOwner,
        listener: OpenGpuiPortListener,
    ) {
        let mut state = self.state.borrow_mut();
        if !state.terminal {
            state
                .keyboard_ports
                .insert(owner.port_instance(), (owner, listener));
        }
    }

    pub(crate) fn activate_keyboard_port(
        &self,
        port: PortInstanceKey,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let activation = self.state.borrow().keyboard_ports.get(&port).cloned();
        let Some((owner, listener)) = activation else {
            return false;
        };
        listener(
            OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
                owner,
                OpenGpuiPortActivationSource::Keyboard,
            )),
            window,
            cx,
        );
        true
    }

    fn mark_overlay_active(&self) {
        let mut state = self.state.borrow_mut();
        if !state.terminal {
            state.overlay_active = true;
            state.space_pan_held = false;
        }
    }

    fn mark_overlay_focus_domain(&self, focus: &FocusHandle) {
        let mut state = self.state.borrow_mut();
        if !state.terminal {
            state.overlay_focus_domains.push(focus.downgrade());
        }
    }

    pub(crate) fn overlay_active(&self) -> bool {
        self.state.borrow().overlay_active
    }

    pub(crate) fn hold_space_pan(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.terminal || state.overlay_active || state.space_pan_held {
            return false;
        }
        state.space_pan_held = true;
        true
    }

    pub(crate) fn space_pan_held(&self) -> bool {
        self.state.borrow().space_pan_held
    }

    pub(crate) fn clear_space_pan(&self) -> bool {
        std::mem::take(&mut self.state.borrow_mut().space_pan_held)
    }

    pub(crate) fn active_owner(&self) -> Option<OpenGpuiPortDragOwner> {
        self.state
            .borrow()
            .capture
            .active()
            .and_then(|active| match active {
                NativeCapture::Port(active) => Some(active.owner.clone()),
                NativeCapture::Scene(_) => None,
            })
    }

    pub(crate) fn requires_window_cleanup(&self) -> bool {
        let state = self.state.borrow();
        state.capture.requires_cleanup()
            || state.port_focus.is_some()
            || !state.overlay_focus_domains.is_empty()
    }

    pub(crate) fn settle_port_focus(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        exit: OpenGpuiPortFocusExit,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let (ownership, overlay_focus_domains) = {
            let mut state = self.state.borrow_mut();
            let ownership = state
                .port_focus
                .take_if(|current| current.owner == *owner && current.mount.is_same_mount(mount));
            (ownership, state.overlay_focus_domains.clone())
        };
        let Some(ownership) = ownership else {
            return false;
        };
        settle_native_focus(ownership.focus, &overlay_focus_domains, exit, window, cx);
        true
    }

    pub(crate) fn validate_port_capture(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !mount.is_live() {
            return;
        }
        let lost = {
            let mut state = self.state.borrow_mut();
            let handle = match state.capture.active() {
                Some(NativeCapture::Port(active))
                    if &active.owner == owner && active.mount.is_same_mount(mount) =>
                {
                    active.handle
                }
                _ => return,
            };
            if !window.captured_pointer().is_some_and(|capture| {
                capture.handle() == handle && capture.button() == MouseButton::Left
            }) {
                match state.capture.take_active() {
                    Some(NativeCapture::Port(active)) => Some(active),
                    _ => unreachable!("matched port capture"),
                }
            } else {
                let active_mount = match state.capture.active_mut() {
                    Some(NativeCapture::Port(active)) => active.mount.clone(),
                    _ => unreachable!("matched port capture"),
                };
                if let Some(focus) = state.port_focus.as_mut()
                    && focus.owner == *owner
                    && focus.mount.is_same_mount(&active_mount)
                {
                    focus.mount = mount.clone();
                }
                None
            }
        };
        if let Some(active) = lost {
            self.queue_port_cancellation_without_suppression(
                active,
                OpenGpuiPortDragCancelReason::PointerCaptureLost,
            );
            self.schedule_pending_drain(window, cx);
        }
    }

    pub(crate) fn begin_port_drag(
        &self,
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        listener: OpenGpuiPortListener,
        activity: OpenGpuiNodeActivityRegistry,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if event.button != MouseButton::Left || !mount.is_live() {
            return false;
        }
        let interaction_busy = {
            let state = self.state.borrow();
            state.terminal || state.capture.is_busy() || state.port_focus.is_some()
        };
        if interaction_busy {
            return true;
        }

        let Some(generation) = self.allocate_generation() else {
            dispatch_cancelled_start(
                &owner,
                &listener,
                OpenGpuiPortDragCancelReason::InteractionUnavailable,
                None,
                event.position,
                event.modifiers,
                window,
                cx,
            );
            return true;
        };
        let weak_state = Rc::downgrade(&self.state);
        let lease =
            activity.acquire_with_revocation(OpenGpuiActivityKind::GraphInteraction, move || {
                invalidate_native_generation(&weak_state, generation);
            });
        let lease = match lease {
            Ok(lease) => lease,
            Err(error) => {
                dispatch_cancelled_start(
                    &owner,
                    &listener,
                    cancel_reason_for_acquire_error(error),
                    Some(NativeInteractionGeneration(generation)),
                    event.position,
                    event.modifiers,
                    window,
                    cx,
                );
                return true;
            }
        };

        let handle = self.pointer_capture_handle(window);
        if let Err(error) = window.capture_pointer(&handle, event.button) {
            drop(lease);
            let reason = match error {
                PointerCaptureError::PointerAlreadyCaptured { .. } => {
                    OpenGpuiPortDragCancelReason::PointerCaptureLost
                }
                _ => OpenGpuiPortDragCancelReason::InteractionUnavailable,
            };
            dispatch_cancelled_start(
                &owner,
                &listener,
                reason,
                Some(NativeInteractionGeneration(generation)),
                event.position,
                event.modifiers,
                window,
                cx,
            );
            return true;
        }
        let previous_focus = window.focused(cx).map(|focus| focus.downgrade());
        let interaction_focus = self.state.borrow().focus_handle.clone();
        let previous_focus_was_graph_domain = interaction_focus
            .as_ref()
            .is_some_and(|focus| focus.contains_focused(window, cx));
        if let Some(focus_handle) = &interaction_focus {
            window.focus(focus_handle, cx);
        }

        let start = OpenGpuiPortDragEvent {
            owner: owner.clone(),
            native_generation: Some(NativeInteractionGeneration(generation)),
            phase: OpenGpuiPortDragPhase::Start,
            position: event.position,
            modifiers: event.modifiers,
        };
        {
            let mut state = self.state.borrow_mut();
            state.port_focus = Some(PortFocusOwnership {
                owner: owner.clone(),
                mount: mount.clone(),
                focus: NativeFocusOwnership {
                    previous_focus,
                    previous_focus_was_graph_domain,
                    interaction_focus: interaction_focus.map(|focus| focus.downgrade()),
                },
            });
            state.capture.activate(NativeCapture::Port(ActivePortDrag {
                generation,
                owner,
                mount,
                listener: listener.clone(),
                lease,
                handle,
                position: event.position,
                modifiers: event.modifiers,
            }));
        }
        listener(OpenGpuiPortEvent::Pointer(start), window, cx);
        window.refresh();
        true
    }

    pub(crate) fn handle_mouse_down(
        &self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        let state = self.state.borrow();
        if state.capture.active().is_some() {
            return true;
        }
        drop(state);
        let suppression = self.release_suppression_for_window(window);
        if suppression.is_some_and(|suppression| suppression.button == event.button) {
            let mut state = self.state.borrow_mut();
            state.capture.set_suppression(None);
            return false;
        }
        suppression.is_some()
    }

    pub(crate) fn handle_mouse_move(
        &self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        let dispatch = {
            let mut state = self.state.borrow_mut();
            match state.capture.active_mut() {
                Some(NativeCapture::Port(active)) => {
                    active.position = event.position;
                    active.modifiers = event.modifiers;
                    Some(NativeMoveDispatch::Port {
                        listener: active.listener.clone(),
                        event: OpenGpuiPortDragEvent {
                            owner: active.owner.clone(),
                            native_generation: Some(NativeInteractionGeneration(active.generation)),
                            phase: OpenGpuiPortDragPhase::Move,
                            position: event.position,
                            modifiers: event.modifiers,
                        },
                    })
                }
                Some(NativeCapture::Scene(active)) => {
                    active.position = event.position;
                    active.modifiers = event.modifiers;
                    Some(NativeMoveDispatch::Scene {
                        listener: active.listener.clone(),
                        event: OpenGpuiSceneGestureEvent {
                            kind: active.kind.clone(),
                            phase: OpenGpuiSceneGesturePhase::Move,
                            position: event.position,
                            modifiers: event.modifiers,
                        },
                    })
                }
                None => None,
            }
        };
        if let Some(dispatch) = dispatch {
            dispatch.send(window, cx);
            window.refresh();
            return true;
        }
        self.release_suppression_for_window(window).is_some()
    }

    pub(crate) fn handle_mouse_up(
        &self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        let active_button = self
            .state
            .borrow()
            .capture
            .active()
            .map(NativeCapture::button);
        if let Some(button) = active_button {
            if button != event.button {
                return true;
            }
            let active = self.state.borrow_mut().capture.take_active();
            if let Some(active) = active {
                let generation = active.generation();
                self.finish_native_capture(
                    active,
                    NativeCaptureExit::Release,
                    event.position,
                    event.modifiers,
                    window,
                    cx,
                );
                self.set_release_suppression_for_generation(generation, None);
            }
            return true;
        }

        let suppress = self
            .release_suppression_for_window(window)
            .is_some_and(|suppression| suppression.button == event.button);
        if suppress {
            self.state.borrow_mut().capture.set_suppression(None);
        }
        suppress
    }

    pub(crate) fn handle_scroll(
        &self,
        _event: &ScrollEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        self.state.borrow().capture.active().is_some()
            || self.release_suppression_for_window(window).is_some()
    }

    pub(crate) fn handle_key_down(
        &self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.release_suppression_for_window(window);
        if !event.keystroke.key.eq_ignore_ascii_case("escape") {
            return false;
        }
        self.drain_pending(window, cx);
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            let position = active.position();
            let modifiers = active.modifiers();
            let suppression = active.release_suppression();
            let generation = active.generation();
            self.finish_native_capture(
                active,
                NativeCaptureExit::Escape,
                position,
                modifiers,
                window,
                cx,
            );
            self.set_release_suppression_for_generation(generation, Some(suppression));
            return true;
        }
        false
    }

    pub(crate) fn disable_port_deferred(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &Window,
        cx: &mut App,
    ) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_some_and(|active| {
                    matches!(active, NativeCapture::Port(active) if &active.owner == owner && active.mount.is_same_mount(mount))
                }) {
                    match state.capture.take_active() {
                        Some(NativeCapture::Port(active)) => Some(active),
                        _ => unreachable!("matched port capture"),
                    }
                } else {
                    None
                }
        };
        let Some(active) = active else {
            return false;
        };

        self.queue_port_cancellation(active, OpenGpuiPortDragCancelReason::PortDisabled);
        self.schedule_pending_drain(window, cx);
        true
    }

    pub(crate) fn cancel_port_owner_deferred(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        reason: OpenGpuiPortDragCancelReason,
    ) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_some_and(|active| {
                    matches!(active, NativeCapture::Port(active) if &active.owner == owner && active.mount.is_same_mount(mount))
                }) {
                    match state.capture.take_active() {
                        Some(NativeCapture::Port(active)) => Some(active),
                        _ => unreachable!("matched port capture"),
                    }
                } else {
                    None
                }
        };
        let Some(active) = active else {
            return false;
        };
        self.queue_port_cancellation(active, reason);
        true
    }

    pub(crate) fn reject_port_start(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        reason: OpenGpuiPortDragCancelReason,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_some_and(|active| {
                    matches!(active, NativeCapture::Port(active) if &active.owner == owner && active.mount.is_same_mount(mount))
                }) {
                    match state.capture.take_active() {
                        Some(NativeCapture::Port(active)) => Some(active),
                        _ => unreachable!("matched port capture"),
                    }
                } else {
                    None
                }
        };
        let Some(active) = active else {
            return false;
        };
        self.queue_port_cancellation_with_focus(
            active,
            reason,
            Some(PendingPortFocusSettlement {
                owner: owner.clone(),
                mount: mount.clone(),
                exit: OpenGpuiPortFocusExit::Restore,
            }),
        );
        self.schedule_pending_drain(window, cx);
        true
    }

    pub(crate) fn drain_pending(&self, window: &mut Window, cx: &mut App) -> bool {
        let pending = self.state.borrow_mut().capture.take_pending();
        let pending = pending
            .into_iter()
            .filter(|pending| native_cancellation_is_current(&self.state, pending.generation()))
            .collect::<VecDeque<_>>();
        if pending.is_empty() {
            self.release_suppression_for_window(window);
            return false;
        }

        release_matching_pointer_capture(&pending, window);
        for pending in pending {
            let PendingNativeCancellation::Port(pending) = pending else {
                let PendingNativeCancellation::Scene(pending) = pending else {
                    unreachable!();
                };
                let interaction_focus = self.state.borrow().focus_handle.clone();
                settle_scene_focus(
                    pending.event.phase(),
                    pending.previous_focus.as_ref(),
                    interaction_focus.as_ref(),
                    window,
                    cx,
                );
                (pending.listener)(pending.event, window, cx);
                continue;
            };
            let PendingPortCancellation {
                generation: _,
                handle: _,
                event,
                listener,
                focus_settlement,
            } = *pending;
            listener(OpenGpuiPortEvent::Pointer(event), window, cx);
            if let Some(focus) = focus_settlement {
                self.settle_port_focus(&focus.owner, &focus.mount, focus.exit, window, cx);
            }
        }
        self.release_suppression_for_window(window);
        window.refresh();
        true
    }

    pub(crate) fn schedule_pending_drain(&self, window: &Window, cx: &mut App) -> bool {
        let should_schedule = self.state.borrow_mut().capture.prepare_pending_drain();
        if !should_schedule {
            return false;
        }

        let handle = self.clone();
        window.defer(cx, move |window, cx| {
            handle.drain_pending(window, cx);
        });
        true
    }

    fn allocate_generation(&self) -> Option<u64> {
        self.state.borrow_mut().capture.allocate_generation()
    }

    fn finish_port_capture(
        &self,
        active: ActivePortDrag,
        phase: OpenGpuiPortDragPhase,
        position: Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) {
        let handle = active.handle;
        let event = OpenGpuiPortDragEvent {
            owner: active.owner,
            native_generation: Some(NativeInteractionGeneration(active.generation)),
            phase,
            position,
            modifiers,
        };
        let listener = active.listener;
        drop(active.lease);
        release_pointer_capture_if_owned(handle, window);
        listener(OpenGpuiPortEvent::Pointer(event), window, cx);
        window.refresh();
    }

    fn queue_port_cancellation(
        &self,
        active: ActivePortDrag,
        reason: OpenGpuiPortDragCancelReason,
    ) {
        self.queue_port_cancellation_with_focus(active, reason, None);
    }

    fn queue_port_cancellation_without_suppression(
        &self,
        active: ActivePortDrag,
        reason: OpenGpuiPortDragCancelReason,
    ) {
        self.queue_port_cancellation_inner(active, reason, None, false);
    }

    fn queue_port_cancellation_with_focus(
        &self,
        active: ActivePortDrag,
        reason: OpenGpuiPortDragCancelReason,
        focus_settlement: Option<PendingPortFocusSettlement>,
    ) {
        self.queue_port_cancellation_inner(active, reason, focus_settlement, true);
    }

    fn queue_port_cancellation_inner(
        &self,
        active: ActivePortDrag,
        reason: OpenGpuiPortDragCancelReason,
        focus_settlement: Option<PendingPortFocusSettlement>,
        suppress_release: bool,
    ) {
        let suppression = suppress_release.then(|| {
            NativeReleaseSuppression::new(MouseButton::Left, active.generation, active.handle)
        });
        let event = OpenGpuiPortDragEvent {
            owner: active.owner,
            native_generation: Some(NativeInteractionGeneration(active.generation)),
            phase: OpenGpuiPortDragPhase::Cancel(reason),
            position: active.position,
            modifiers: active.modifiers,
        };
        drop(active.lease);
        self.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Port(Box::new(PendingPortCancellation {
                generation: active.generation,
                handle: active.handle,
                event,
                listener: active.listener,
                focus_settlement,
            })),
            suppression,
        );
    }
}

/// Reason a node/canvas gesture was cancelled before release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiSceneGestureCancelReason {
    Escape,
    OwnerRetired,
    HostClosed,
    ProtectedCapacityExceeded,
    InteractionUnavailable,
    PointerCaptureLost,
}

/// Scene-level gesture owner. Native controls and real ports are handled by their own layers
/// before these owners are allowed to claim the pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiSceneGestureKind {
    Node(NodeInstanceKey),
    EdgeReconnect(EdgeInstanceKey, EdgeEndpoint),
    Pan,
    Selection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiSceneGesturePhase {
    Start,
    Move,
    Release,
    Cancel(OpenGpuiSceneGestureCancelReason),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OpenGpuiSceneGestureEvent {
    kind: OpenGpuiSceneGestureKind,
    phase: OpenGpuiSceneGesturePhase,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

impl OpenGpuiSceneGestureEvent {
    pub(crate) fn kind(&self) -> &OpenGpuiSceneGestureKind {
        &self.kind
    }

    pub(crate) fn phase(&self) -> OpenGpuiSceneGesturePhase {
        self.phase
    }

    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }

    pub(crate) fn modifiers(&self) -> Modifiers {
        self.modifiers
    }
}

pub(crate) type OpenGpuiSceneGestureListener =
    Rc<dyn Fn(OpenGpuiSceneGestureEvent, &mut Window, &mut App) + 'static>;

struct ActiveSceneInteraction {
    generation: u64,
    kind: OpenGpuiSceneGestureKind,
    button: MouseButton,
    listener: OpenGpuiSceneGestureListener,
    lease: Option<OpenGpuiActivityLease>,
    handle: PointerCaptureHandle,
    previous_focus: Option<WeakFocusHandle>,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

struct PendingSceneCancellation {
    generation: u64,
    handle: PointerCaptureHandle,
    button: MouseButton,
    event: OpenGpuiSceneGestureEvent,
    listener: OpenGpuiSceneGestureListener,
    previous_focus: Option<WeakFocusHandle>,
}

enum NativeMoveDispatch {
    Port {
        listener: OpenGpuiPortListener,
        event: OpenGpuiPortDragEvent,
    },
    Scene {
        listener: OpenGpuiSceneGestureListener,
        event: OpenGpuiSceneGestureEvent,
    },
}

impl NativeMoveDispatch {
    fn send(self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Port { listener, event } => {
                listener(OpenGpuiPortEvent::Pointer(event), window, cx);
            }
            Self::Scene { listener, event } => listener(event, window, cx),
        }
    }
}

enum NativeCaptureExit {
    Release,
    Escape,
}

impl NativeCapture {
    fn button(&self) -> MouseButton {
        match self {
            Self::Port(_) => MouseButton::Left,
            Self::Scene(active) => active.button,
        }
    }

    fn position(&self) -> Point<Pixels> {
        match self {
            Self::Port(active) => active.position,
            Self::Scene(active) => active.position,
        }
    }

    fn modifiers(&self) -> Modifiers {
        match self {
            Self::Port(active) => active.modifiers,
            Self::Scene(active) => active.modifiers,
        }
    }

    fn handle(&self) -> PointerCaptureHandle {
        match self {
            Self::Port(active) => active.handle,
            Self::Scene(active) => active.handle,
        }
    }

    fn release_suppression(&self) -> NativeReleaseSuppression {
        NativeReleaseSuppression::new(self.button(), self.generation(), self.handle())
    }
}

impl OpenGpuiNativeInteractionHandle {
    pub(crate) fn begin(
        &self,
        kind: OpenGpuiSceneGestureKind,
        button: MouseButton,
        listener: OpenGpuiSceneGestureListener,
        activity: Option<OpenGpuiNodeActivityRegistry>,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if event.button != button {
            return false;
        }
        let state = self.state.borrow();
        if state.terminal || state.capture.is_busy() || state.port_focus.is_some() {
            return true;
        }
        drop(state);
        let Some(generation) = self.allocate_generation() else {
            self.dispatch_cancelled_start(
                kind,
                listener,
                OpenGpuiSceneGestureCancelReason::InteractionUnavailable,
                event,
                window,
                cx,
            );
            return true;
        };
        let lease = if let Some(activity) = activity {
            let weak_state = Rc::downgrade(&self.state);
            match activity
                .acquire_with_revocation(OpenGpuiActivityKind::GraphInteraction, move || {
                    invalidate_native_generation(&weak_state, generation)
                }) {
                Ok(lease) => Some(lease),
                Err(error) => {
                    self.dispatch_cancelled_start(
                        kind,
                        listener,
                        scene_cancel_reason_for_acquire_error(error),
                        event,
                        window,
                        cx,
                    );
                    return true;
                }
            }
        } else {
            None
        };
        let handle = self.pointer_capture_handle(window);
        if let Err(error) = window.capture_pointer(&handle, button) {
            drop(lease);
            let reason = match error {
                PointerCaptureError::PointerAlreadyCaptured { .. } => {
                    OpenGpuiSceneGestureCancelReason::PointerCaptureLost
                }
                _ => OpenGpuiSceneGestureCancelReason::InteractionUnavailable,
            };
            self.dispatch_cancelled_start(kind, listener, reason, event, window, cx);
            return true;
        }
        let previous_focus = window.focused(cx).map(|focus| focus.downgrade());
        if let Some(focus_handle) = self.state.borrow().focus_handle.clone() {
            window.focus(&focus_handle, cx);
        }
        let start = OpenGpuiSceneGestureEvent {
            kind: kind.clone(),
            phase: OpenGpuiSceneGesturePhase::Start,
            position: event.position,
            modifiers: event.modifiers,
        };
        self.state
            .borrow_mut()
            .capture
            .activate(NativeCapture::Scene(ActiveSceneInteraction {
                generation,
                kind,
                button,
                listener: listener.clone(),
                lease,
                handle,
                previous_focus,
                position: event.position,
                modifiers: event.modifiers,
            }));
        listener(start, window, cx);
        window.refresh();
        true
    }

    pub(crate) fn handle_pinch(&self, window: &mut Window, cx: &mut App) -> bool {
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        self.state.borrow().capture.active().is_some()
            || self.release_suppression_for_window(window).is_some()
    }

    fn handle_pointer_cancel(&self, window: &mut Window, cx: &mut App) {
        let active = {
            let mut state = self.state.borrow_mut();
            state.space_pan_held = false;
            state.capture.set_suppression(None);
            state.capture.take_active()
        };
        if let Some(active) = active {
            match active {
                NativeCapture::Port(active) => self.queue_port_cancellation_without_suppression(
                    active,
                    OpenGpuiPortDragCancelReason::PointerCaptureLost,
                ),
                NativeCapture::Scene(active) => self.queue_scene_cancellation_without_suppression(
                    active,
                    OpenGpuiSceneGestureCancelReason::PointerCaptureLost,
                ),
            }
        }
        self.drain_pending(window, cx);
    }

    pub(crate) fn cancel_scene_deferred(&self, reason: OpenGpuiSceneGestureCancelReason) -> bool {
        let active = self.take_scene_capture_if(|_| true);
        let Some(active) = active else {
            return false;
        };
        self.queue_scene_cancellation(active, reason);
        true
    }

    pub(crate) fn cancel_scene_owner_deferred(
        &self,
        owner: &NodeInstanceKey,
        reason: OpenGpuiSceneGestureCancelReason,
    ) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_some_and(|active| {
                matches!(active, NativeCapture::Scene(active) if matches!(&active.kind, OpenGpuiSceneGestureKind::Node(current) if current == owner))
            }) {
                match state.capture.take_active() {
                    Some(NativeCapture::Scene(active)) => Some(active),
                    _ => unreachable!("matched scene capture"),
                }
            } else {
                None
            }
        };
        let Some(active) = active else {
            return false;
        };
        self.queue_scene_cancellation(active, reason);
        true
    }
}

impl OpenGpuiNativeInteractionAuthority {
    pub(crate) fn prepare_host_close_deferred(&self) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.terminal {
                return false;
            }
            state.terminal = true;
            state.focus_handle = None;
            state.keyboard_ports.clear();
            state.overlay_active = false;
            state.space_pan_held = false;
            state.capture.set_suppression(None);
            state.capture.take_active()
        };
        let had_active = active.is_some();
        if let Some(active) = active {
            match active {
                NativeCapture::Port(active) => {
                    self.queue_port_cancellation(active, OpenGpuiPortDragCancelReason::HostClosed)
                }
                NativeCapture::Scene(active) => self
                    .queue_scene_cancellation(active, OpenGpuiSceneGestureCancelReason::HostClosed),
            }
        }
        had_active
    }

    pub(crate) fn cancel_for_host_close(&self, window: &mut Window, cx: &mut App) -> bool {
        let prepared = self.prepare_host_close_deferred();
        let (pending, port_focus, overlay_focus_domains) = {
            let mut state = self.state.borrow_mut();
            (
                state.capture.take_pending(),
                state.port_focus.take(),
                std::mem::take(&mut state.overlay_focus_domains),
            )
        };
        let focus_owned = port_focus.is_some() || !overlay_focus_domains.is_empty();
        if let Some(port_focus) = port_focus {
            settle_native_focus(
                port_focus.focus,
                &overlay_focus_domains,
                OpenGpuiPortFocusExit::HostClosed,
                window,
                cx,
            );
        } else if focus_is_owned_by_domains(&overlay_focus_domains, window, cx) {
            window.blur(cx);
        }
        if !prepared && pending.is_empty() && !focus_owned {
            return false;
        }
        release_matching_pointer_capture(&pending, window);
        window.refresh();
        if pending.is_empty() {
            return prepared || focus_owned;
        }
        let state = Rc::downgrade(&self.state);
        window.defer(cx, move |window, cx| {
            for pending in pending {
                let Some(state) = state.upgrade() else {
                    return;
                };
                if !native_cancellation_is_current(&state, pending.generation()) {
                    continue;
                }
                match pending {
                    PendingNativeCancellation::Port(pending) => {
                        (pending.listener)(OpenGpuiPortEvent::Pointer(pending.event), window, cx);
                    }
                    PendingNativeCancellation::Scene(pending) => {
                        (pending.listener)(pending.event, window, cx);
                    }
                }
            }
            window.refresh();
        });
        true
    }
}

fn release_matching_pointer_capture(
    pending: &VecDeque<PendingNativeCancellation>,
    window: &mut Window,
) -> bool {
    let Some(captured) = window.captured_pointer() else {
        return false;
    };
    pending.iter().any(|pending| {
        pending.handle() == captured.handle() && pending.button() == captured.button()
    }) && release_pointer_capture_if_owned(captured.handle(), window)
}

pub(crate) fn release_pointer_capture_if_owned(
    handle: PointerCaptureHandle,
    window: &mut Window,
) -> bool {
    window.release_pointer(&handle).unwrap_or(false)
}

impl OpenGpuiNativeInteractionHandle {
    fn dispatch_cancelled_start(
        &self,
        kind: OpenGpuiSceneGestureKind,
        listener: OpenGpuiSceneGestureListener,
        reason: OpenGpuiSceneGestureCancelReason,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        listener(
            OpenGpuiSceneGestureEvent {
                kind,
                phase: OpenGpuiSceneGesturePhase::Cancel(reason),
                position: event.position,
                modifiers: event.modifiers,
            },
            window,
            cx,
        );
    }

    fn finish_scene_capture(
        &self,
        active: ActiveSceneInteraction,
        phase: OpenGpuiSceneGesturePhase,
        position: Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) {
        let handle = active.handle;
        let event = OpenGpuiSceneGestureEvent {
            kind: active.kind,
            phase,
            position,
            modifiers,
        };
        let previous_focus = active.previous_focus;
        let interaction_focus = self.state.borrow().focus_handle.clone();
        drop(active.lease);
        release_pointer_capture_if_owned(handle, window);
        settle_scene_focus(
            phase,
            previous_focus.as_ref(),
            interaction_focus.as_ref(),
            window,
            cx,
        );
        (active.listener)(event, window, cx);
        window.refresh();
    }

    fn queue_scene_cancellation(
        &self,
        active: ActiveSceneInteraction,
        reason: OpenGpuiSceneGestureCancelReason,
    ) {
        self.queue_scene_cancellation_inner(active, reason, true);
    }

    fn queue_scene_cancellation_without_suppression(
        &self,
        active: ActiveSceneInteraction,
        reason: OpenGpuiSceneGestureCancelReason,
    ) {
        self.queue_scene_cancellation_inner(active, reason, false);
    }

    fn queue_scene_cancellation_inner(
        &self,
        active: ActiveSceneInteraction,
        reason: OpenGpuiSceneGestureCancelReason,
        suppress_release: bool,
    ) {
        let event = OpenGpuiSceneGestureEvent {
            kind: active.kind,
            phase: OpenGpuiSceneGesturePhase::Cancel(reason),
            position: active.position,
            modifiers: active.modifiers,
        };
        let suppression = suppress_release.then(|| {
            NativeReleaseSuppression::new(active.button, active.generation, active.handle)
        });
        let previous_focus = active.previous_focus;
        drop(active.lease);
        self.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Scene(PendingSceneCancellation {
                generation: active.generation,
                handle: active.handle,
                button: active.button,
                event,
                listener: active.listener,
                previous_focus,
            }),
            suppression,
        );
    }

    fn cancel_replaced_capture_deferred(&self, window: &Window, cx: &mut App) -> bool {
        let captured_pointer = window.captured_pointer();
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_none_or(|active| {
                captured_pointer.is_some_and(|capture| {
                    capture.handle() == active.handle() && capture.button() == active.button()
                })
            }) {
                return false;
            }
            state.capture.take_active()
        };
        let Some(active) = active else {
            return false;
        };
        match active {
            NativeCapture::Port(active) => self.queue_port_cancellation_without_suppression(
                active,
                OpenGpuiPortDragCancelReason::PointerCaptureLost,
            ),
            NativeCapture::Scene(active) => self.queue_scene_cancellation_without_suppression(
                active,
                OpenGpuiSceneGestureCancelReason::PointerCaptureLost,
            ),
        }
        self.schedule_pending_drain(window, cx);
        true
    }

    fn release_suppression_for_window(&self, window: &Window) -> Option<NativeReleaseSuppression> {
        let mut state = self.state.borrow_mut();
        let suppression = state.capture.suppression()?;
        let is_current_generation = state.capture.latest_generation() == suppression.generation;
        let capture_is_unclaimed_or_owned = window.captured_pointer().is_none_or(|captured| {
            captured.handle() == suppression.handle && captured.button() == suppression.button
        });
        if is_current_generation && capture_is_unclaimed_or_owned {
            return Some(suppression);
        }
        state.capture.set_suppression(None);
        None
    }

    fn set_release_suppression_for_generation(
        &self,
        generation: u64,
        suppression: Option<NativeReleaseSuppression>,
    ) -> bool {
        let mut state = self.state.borrow_mut();
        if state.terminal || state.capture.latest_generation() != generation {
            return false;
        }
        state.capture.set_suppression(suppression);
        true
    }

    fn finish_native_capture(
        &self,
        active: NativeCapture,
        exit: NativeCaptureExit,
        position: Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) {
        match active {
            NativeCapture::Port(active) => {
                let phase = match exit {
                    NativeCaptureExit::Release => OpenGpuiPortDragPhase::Release,
                    NativeCaptureExit::Escape => {
                        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape)
                    }
                };
                self.finish_port_capture(active, phase, position, modifiers, window, cx);
            }
            NativeCapture::Scene(active) => {
                let phase = match exit {
                    NativeCaptureExit::Release => OpenGpuiSceneGesturePhase::Release,
                    NativeCaptureExit::Escape => {
                        OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::Escape)
                    }
                };
                self.finish_scene_capture(active, phase, position, modifiers, window, cx);
            }
        }
    }

    fn take_scene_capture_if(
        &self,
        predicate: impl FnOnce(&ActiveSceneInteraction) -> bool,
    ) -> Option<ActiveSceneInteraction> {
        let mut state = self.state.borrow_mut();
        let should_take = matches!(
            state.capture.active(),
            Some(NativeCapture::Scene(active)) if predicate(active)
        );
        if !should_take {
            return None;
        }
        match state.capture.take_active() {
            Some(NativeCapture::Scene(active)) => Some(active),
            _ => unreachable!("matched scene capture"),
        }
    }
}

fn scene_cancel_reason_for_acquire_error(
    error: OpenGpuiActivityAcquireError,
) -> OpenGpuiSceneGestureCancelReason {
    match error {
        OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. } => {
            OpenGpuiSceneGestureCancelReason::ProtectedCapacityExceeded
        }
        OpenGpuiActivityAcquireError::RegistryRevoked
        | OpenGpuiActivityAcquireError::IdentityExhausted => {
            OpenGpuiSceneGestureCancelReason::InteractionUnavailable
        }
    }
}

fn invalidate_native_generation(state: &Weak<RefCell<NativeInteractionState>>, generation: u64) {
    let Some(state) = state.upgrade() else {
        return;
    };
    let active = state
        .borrow_mut()
        .capture
        .take_active_for_generation(generation);
    let Some(active) = active else {
        return;
    };
    match active {
        NativeCapture::Port(active) => {
            let suppression =
                NativeReleaseSuppression::new(MouseButton::Left, active.generation, active.handle);
            let event = OpenGpuiPortDragEvent {
                owner: active.owner,
                native_generation: Some(NativeInteractionGeneration(active.generation)),
                phase: OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
                position: active.position,
                modifiers: active.modifiers,
            };
            drop(active.lease);
            state.borrow_mut().capture.enqueue(
                PendingNativeCancellation::Port(Box::new(PendingPortCancellation {
                    generation: active.generation,
                    handle: active.handle,
                    event,
                    listener: active.listener,
                    focus_settlement: None,
                })),
                Some(suppression),
            );
        }
        NativeCapture::Scene(active) => {
            let suppression =
                NativeReleaseSuppression::new(active.button, active.generation, active.handle);
            let event = OpenGpuiSceneGestureEvent {
                kind: active.kind,
                phase: OpenGpuiSceneGesturePhase::Cancel(
                    OpenGpuiSceneGestureCancelReason::OwnerRetired,
                ),
                position: active.position,
                modifiers: active.modifiers,
            };
            let previous_focus = active.previous_focus;
            drop(active.lease);
            state.borrow_mut().capture.enqueue(
                PendingNativeCancellation::Scene(PendingSceneCancellation {
                    generation: active.generation,
                    handle: active.handle,
                    button: active.button,
                    event,
                    listener: active.listener,
                    previous_focus,
                }),
                Some(suppression),
            );
        }
    }
}

fn native_cancellation_is_current(
    state: &Rc<RefCell<NativeInteractionState>>,
    generation: u64,
) -> bool {
    state.borrow().capture.latest_generation() == generation
}

pub(crate) fn interaction_root(
    id: impl Into<ElementId>,
    interactions: OpenGpuiNativeInteractionHandle,
    native_input: OpenGpuiNativeInputHandle,
    focus_handle: FocusHandle,
    graph_key_down_listener: Option<OpenGpuiGraphKeyDownListener>,
    graph_key_up_listener: Option<OpenGpuiGraphKeyUpListener>,
    child: impl IntoElement,
) -> impl IntoElement {
    let child = div()
        .size_full()
        .track_focus(&focus_handle)
        .tab_stop(true)
        .child(child);
    OpenGpuiInteractionRoot {
        id: id.into(),
        interactions,
        native_input,
        focus_handle,
        graph_key_down_listener,
        graph_key_up_listener,
        child: Some(child.into_any_element()),
    }
}

pub(crate) type OpenGpuiGraphKeyDownListener =
    Rc<dyn Fn(OpenGpuiGraphKeyOwner, &KeyDownEvent, &mut Window, &mut App) -> bool + 'static>;
pub(crate) type OpenGpuiGraphKeyUpListener =
    Rc<dyn Fn(OpenGpuiGraphKeyOwner, &KeyUpEvent, &mut Window, &mut App) -> bool + 'static>;

struct OpenGpuiInteractionRoot {
    id: ElementId,
    interactions: OpenGpuiNativeInteractionHandle,
    native_input: OpenGpuiNativeInputHandle,
    focus_handle: FocusHandle,
    graph_key_down_listener: Option<OpenGpuiGraphKeyDownListener>,
    graph_key_up_listener: Option<OpenGpuiGraphKeyUpListener>,
    child: Option<AnyElement>,
}

impl Element for OpenGpuiInteractionRoot {
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
        let mut child = self.child.take().expect("interaction root child missing");
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.interactions.schedule_pending_drain(window, cx);
        self.interactions
            .cancel_replaced_capture_deferred(window, cx);
        window.set_focus_handle(&self.focus_handle, cx);
        child.prepaint(window, cx);
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
        self.interactions.begin_graph_paint();
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_mouse_down(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_mouse_move(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_mouse_up(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_scroll(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let interactions = self.interactions.clone();
        window.on_mouse_event(move |_: &PinchEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_pinch(window, cx) {
                cx.stop_propagation();
            }
        });
        let interactions = self.interactions.clone();
        window.on_pointer_cancel(move |_: &PointerCancelEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture {
                interactions.handle_pointer_cancel(window, cx);
            }
        });
        let interactions = self.interactions.clone();
        let native_input = self.native_input.clone();
        let graph_key_down_listener = self.graph_key_down_listener.clone();
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                if native_input.take_native_tab_fallback(&event.keystroke) {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                }
                return;
            }
            if phase != DispatchPhase::Capture {
                return;
            }
            let owner = native_input.take_key_down(&event.keystroke);
            if owner == OpenGpuiNativeKeyOwner::NativeControl && is_unmodified_tab(event) {
                native_input.defer_native_tab_fallback(&event.keystroke);
                return;
            }
            if let OpenGpuiNativeKeyOwner::Graph(graph_owner) = owner
                && is_unmodified_tab(event)
            {
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                if let Some(listener) = graph_key_down_listener.as_ref() {
                    let _ = listener(graph_owner, event, window, cx);
                }
                window.prevent_default();
                cx.stop_propagation();
                return;
            }
            let OpenGpuiNativeKeyOwner::Graph(graph_owner) = owner else {
                return;
            };
            if interactions.handle_key_down(event, window, cx) {
                window.prevent_default();
                cx.stop_propagation();
                return;
            }
            if graph_key_down_listener
                .as_ref()
                .is_some_and(|listener| listener(graph_owner, event, window, cx))
            {
                window.prevent_default();
                cx.stop_propagation();
            }
        });
        if let Some(listener) = self.graph_key_up_listener.clone() {
            let native_input = self.native_input.clone();
            window.on_key_event(move |event: &KeyUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let OpenGpuiNativeKeyOwner::Graph(graph_owner) =
                    native_input.take_key_up(&event.keystroke)
                else {
                    return;
                };
                if listener(graph_owner, event, window, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
        }
        child.paint(window, cx);
    }
}

fn is_unmodified_tab(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    event.keystroke.key.eq_ignore_ascii_case("tab")
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
}

impl IntoElement for OpenGpuiInteractionRoot {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct OpenGpuiOverlayInteractionElement {
    interactions: OpenGpuiNativeInteractionHandle,
    focus_domain: Option<FocusHandle>,
    child: Option<AnyElement>,
}

impl OpenGpuiOverlayInteractionElement {
    fn new(interactions: OpenGpuiNativeInteractionHandle, child: impl IntoElement) -> Self {
        Self {
            interactions,
            focus_domain: None,
            child: Some(child.into_any_element()),
        }
    }

    fn new_with_focus(
        interactions: OpenGpuiNativeInteractionHandle,
        focus_domain: FocusHandle,
        child: impl IntoElement,
    ) -> Self {
        Self {
            interactions,
            focus_domain: Some(focus_domain),
            child: Some(child.into_any_element()),
        }
    }
}

impl Element for OpenGpuiOverlayInteractionElement {
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
        let mut child = self
            .child
            .take()
            .expect("overlay interaction boundary child missing");
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
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::BlockMouse);
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
        self.interactions.mark_overlay_active();
        if let Some(focus_domain) = self.focus_domain.as_ref() {
            self.interactions.mark_overlay_focus_domain(focus_domain);
        }
        let pointer_hitbox = hitbox.clone();
        window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && pointer_hitbox.is_hovered(window) {
                cx.stop_propagation();
            }
        });
        let pointer_hitbox = hitbox.clone();
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && pointer_hitbox.is_hovered(window) {
                cx.stop_propagation();
            }
        });
        let pointer_hitbox = hitbox.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && pointer_hitbox.is_hovered(window) {
                cx.stop_propagation();
            }
        });
        let scroll_hitbox = hitbox.clone();
        window.on_mouse_event(move |_: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && scroll_hitbox.should_handle_scroll(window) {
                cx.stop_propagation();
            }
        });
        let pinch_hitbox = hitbox.clone();
        window.on_mouse_event(move |_: &PinchEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && pinch_hitbox.is_hovered(window) {
                cx.stop_propagation();
            }
        });
        child.paint(window, cx);
    }
}

impl IntoElement for OpenGpuiOverlayInteractionElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn settle_native_focus(
    ownership: NativeFocusOwnership,
    overlay_focus_domains: &[WeakFocusHandle],
    exit: OpenGpuiPortFocusExit,
    window: &mut Window,
    cx: &mut App,
) {
    let previous_focus = ownership
        .previous_focus
        .as_ref()
        .and_then(WeakFocusHandle::upgrade);
    let interaction_focus = ownership
        .interaction_focus
        .as_ref()
        .and_then(WeakFocusHandle::upgrade);
    let current_focus = window.focused(cx);
    let current_is_owned_overlay = focus_is_owned_by_domains(overlay_focus_domains, window, cx);
    let restore_graph_fallback = |window: &mut Window, cx: &mut App| {
        if ownership.previous_focus_was_graph_domain
            && let Some(interaction_focus) = interaction_focus.as_ref()
            && !interaction_focus.contains_focused(window, cx)
        {
            window.focus(interaction_focus, cx);
        }
    };

    match exit {
        OpenGpuiPortFocusExit::Restore | OpenGpuiPortFocusExit::RestoreSearchOverlay => {
            let current_is_previous = current_focus
                .as_ref()
                .zip(previous_focus.as_ref())
                .is_some_and(|(current, previous)| current == previous);
            if current_is_previous {
                restore_graph_fallback(window, cx);
                return;
            }
            let current_is_interaction = current_focus
                .as_ref()
                .zip(interaction_focus.as_ref())
                .is_some_and(|(current, interaction)| current == interaction);
            if current_focus.is_some()
                && !current_is_interaction
                && !(exit == OpenGpuiPortFocusExit::RestoreSearchOverlay
                    && current_is_owned_overlay)
            {
                return;
            }
            if let Some(previous_focus) = previous_focus {
                window.focus(&previous_focus, cx);
                restore_graph_fallback(window, cx);
            } else if let Some(interaction_focus) = interaction_focus {
                window.focus(&interaction_focus, cx);
            } else {
                window.blur(cx);
            }
        }
        OpenGpuiPortFocusExit::OwnerRetired => {
            let current_is_interaction = current_focus
                .as_ref()
                .zip(interaction_focus.as_ref())
                .is_some_and(|(current, interaction)| current == interaction);
            if current_is_interaction || current_is_owned_overlay {
                window.blur(cx);
            }
        }
        OpenGpuiPortFocusExit::HostClosed => {
            if interaction_focus
                .as_ref()
                .is_some_and(|interaction| interaction.contains_focused(window, cx))
                || current_is_owned_overlay
            {
                window.blur(cx);
            }
        }
    }
}

fn focus_is_owned_by_domains(domains: &[WeakFocusHandle], window: &Window, cx: &App) -> bool {
    domains.iter().any(|focus| {
        focus
            .upgrade()
            .is_some_and(|focus| focus.contains_focused(window, cx))
    })
}

fn settle_scene_focus(
    phase: OpenGpuiSceneGesturePhase,
    previous_focus: Option<&WeakFocusHandle>,
    interaction_focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    let interaction_owns_focus =
        interaction_focus.is_some_and(|focus| focus.contains_focused(window, cx));
    if !interaction_owns_focus {
        return;
    }
    if matches!(
        phase,
        OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::HostClosed)
    ) {
        window.blur(cx);
        return;
    }
    if let Some(previous_focus) = previous_focus.and_then(WeakFocusHandle::upgrade) {
        window.focus(&previous_focus, cx);
    }
}

fn cancel_reason_for_acquire_error(
    error: OpenGpuiActivityAcquireError,
) -> OpenGpuiPortDragCancelReason {
    match error {
        OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. } => {
            OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded
        }
        OpenGpuiActivityAcquireError::RegistryRevoked
        | OpenGpuiActivityAcquireError::IdentityExhausted => {
            OpenGpuiPortDragCancelReason::InteractionUnavailable
        }
    }
}

fn dispatch_cancelled_start(
    owner: &OpenGpuiPortDragOwner,
    listener: &OpenGpuiPortListener,
    reason: OpenGpuiPortDragCancelReason,
    native_generation: Option<NativeInteractionGeneration>,
    position: Point<Pixels>,
    modifiers: Modifiers,
    window: &mut Window,
    cx: &mut App,
) {
    listener(
        OpenGpuiPortEvent::Pointer(OpenGpuiPortDragEvent {
            owner: owner.clone(),
            native_generation,
            phase: OpenGpuiPortDragPhase::Cancel(reason),
            position,
            modifiers,
        }),
        window,
        cx,
    );
}

pub(crate) fn port_drag_owner(
    instance: NodeInstanceKey,
    port_instance: PortInstanceKey,
) -> Option<OpenGpuiPortDragOwner> {
    (instance.lifetime() == port_instance.owner()).then_some(OpenGpuiPortDragOwner {
        instance,
        port_instance,
    })
}

#[cfg(test)]
mod capture_bookkeeping_tests {
    use std::cell::Cell;
    use std::fmt::Debug;

    use open_gpui::{Context, Entity, Render, TestAppContext, VisualTestContext, px};

    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct TestActive {
        generation: u64,
    }

    #[derive(Default)]
    struct FocusTestView {
        focus_handles: Vec<FocusHandle>,
        capture_handles: Vec<PointerCaptureHandle>,
        scene: Option<SceneCaptureTestRoot>,
    }

    struct SceneCaptureTestRoot {
        interactions: OpenGpuiNativeInteractionHandle,
        capture_handle: PointerCaptureHandle,
        focus: FocusHandle,
        listener: OpenGpuiSceneGestureListener,
    }

    impl Render for FocusTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            if let Some(scene) = &self.scene {
                let pointer_interactions = scene.interactions.clone();
                let listener = scene.listener.clone();
                return interaction_root(
                    "maintained-scene-capture",
                    scene.interactions.clone(),
                    OpenGpuiNativeInputHandle::default(),
                    scene.focus.clone(),
                    None,
                    None,
                    div()
                        .size_full()
                        .track_pointer_capture(&scene.capture_handle)
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            if pointer_interactions.begin(
                                OpenGpuiSceneGestureKind::Selection,
                                MouseButton::Left,
                                listener.clone(),
                                None,
                                event.window_event(),
                                window,
                                cx,
                            ) {
                                cx.stop_propagation();
                            }
                        }),
                )
                .into_any_element();
            }
            open_gpui::div()
                .relative()
                .size_full()
                .child(
                    open_gpui::div().flex().flex_col().children(
                        self.focus_handles
                            .iter()
                            .enumerate()
                            .map(|(index, handle)| {
                                open_gpui::div()
                                    .id(index)
                                    .track_focus(handle)
                                    .w(px(100.0))
                                    .h(px(24.0))
                            }),
                    ),
                )
                .child(
                    open_gpui::div().id("capture-bindings").children(
                        self.capture_handles
                            .iter()
                            .enumerate()
                            .map(|(index, handle)| {
                                open_gpui::div()
                                    .id(index)
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .w(px(100.0))
                                    .h(px(100.0))
                                    .track_pointer_capture(handle)
                            }),
                    ),
                )
                .into_any_element()
        }
    }

    fn focus_test_window(
        cx: &mut TestAppContext,
    ) -> (Entity<FocusTestView>, &mut VisualTestContext) {
        let (view, cx) = cx.add_window_view(|_, _| FocusTestView::default());
        let _ = cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        (view, cx)
    }

    fn bind_focus_handles(
        view: &Entity<FocusTestView>,
        handles: &[FocusHandle],
        window: &mut Window,
        cx: &mut App,
    ) {
        view.update(cx, |view, cx| {
            view.focus_handles = handles.to_vec();
            cx.notify();
        });
        window.draw(cx).clear(cx);
        for handle in handles {
            assert!(window.is_focus_handle_rendered(handle));
        }
    }

    fn focus_presented(handle: &FocusHandle, window: &mut Window, cx: &mut App) {
        assert!(window.is_focus_handle_rendered(handle));
        window.focus(handle, cx);
        window.draw(cx).clear(cx);
        assert_eq!(window.committed_focus(cx).as_ref(), Some(handle));
    }

    #[test]
    fn overlay_claim_clears_space_pan_and_blocks_rearm_until_the_next_graph_paint() {
        let handle = OpenGpuiNativeInteractionHandle::new();

        assert!(handle.hold_space_pan());
        handle.mark_overlay_active();
        assert!(!handle.space_pan_held());
        assert!(!handle.hold_space_pan());

        handle.begin_graph_paint();
        assert!(handle.hold_space_pan());
    }

    fn draw_capture_handles(
        view: &Entity<FocusTestView>,
        cx: &mut open_gpui::VisualTestContext,
        interactions: &OpenGpuiNativeInteractionHandle,
        button: MouseButton,
    ) -> (PointerCaptureHandle, PointerCaptureHandle) {
        let (owner, successor) = cx.update(|window, cx| {
            let owner = interactions.pointer_capture_handle(window);
            let successor = window.new_pointer_capture_handle();
            view.update(cx, |view, cx| {
                view.capture_handles = vec![owner, successor];
                cx.notify();
            });
            window.draw(cx).clear(cx);
            (owner, successor)
        });
        cx.run_until_parked();
        cx.simulate_event(MouseDownEvent {
            button,
            position: open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0)),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        (owner, successor)
    }

    fn draw_scene_capture_root(
        view: &Entity<FocusTestView>,
        cx: &mut open_gpui::VisualTestContext,
        interactions: &OpenGpuiNativeInteractionHandle,
        listener: OpenGpuiSceneGestureListener,
    ) -> PointerCaptureHandle {
        let capture_handle = cx.update(|window, cx| {
            let capture_handle = interactions.pointer_capture_handle(window);
            let focus = view
                .read(cx)
                .scene
                .as_ref()
                .map(|scene| scene.focus.clone())
                .unwrap_or_else(|| cx.focus_handle());
            view.update(cx, |view, cx| {
                view.scene = Some(SceneCaptureTestRoot {
                    interactions: interactions.clone(),
                    capture_handle,
                    focus,
                    listener,
                });
                cx.notify();
            });
            window.draw(cx).clear(cx);
            capture_handle
        });
        cx.run_until_parked();
        capture_handle
    }

    #[open_gpui::test]
    fn maintained_scene_capture_survives_redraw_and_only_matching_release_finishes(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let interactions = OpenGpuiNativeInteractionHandle::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let listener: OpenGpuiSceneGestureListener = {
            let phases = phases.clone();
            Rc::new(move |event, _, _| phases.borrow_mut().push(event.phase()))
        };
        let capture_handle = draw_scene_capture_root(&view, cx, &interactions, listener.clone());
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0)),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        assert!(cx.update(|window, _| capture_handle.is_captured(window)));
        assert_eq!(
            draw_scene_capture_root(&view, cx, &interactions, listener),
            capture_handle
        );
        assert!(cx.update(|window, _| capture_handle.is_captured(window)));

        let outside = open_gpui::point(open_gpui::px(200.0), open_gpui::px(300.0));
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Right,
            position: outside,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        assert!(cx.update(|window, _| capture_handle.is_captured(window)));
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: outside,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        assert!(cx.update(|window, _| window.captured_pointer().is_none()));
        assert_eq!(
            phases.borrow().as_slice(),
            [
                OpenGpuiSceneGesturePhase::Start,
                OpenGpuiSceneGesturePhase::Release
            ]
        );
    }

    #[open_gpui::test]
    fn maintained_native_pointer_cancellation_finishes_once_without_release_suppression(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let interactions = OpenGpuiNativeInteractionHandle::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let listener: OpenGpuiSceneGestureListener = {
            let phases = phases.clone();
            Rc::new(move |event, _, _| phases.borrow_mut().push(event.phase()))
        };
        draw_scene_capture_root(&view, cx, &interactions, listener);
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0)),
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        assert!(interactions.hold_space_pan());
        let cancellation = PointerCancelEvent {
            reason: open_gpui::PointerCancelReason::PlatformCaptureLost,
        };
        cx.simulate_event(cancellation);
        cx.simulate_event(cancellation);

        assert!(!interactions.space_pan_held());
        assert!(interactions.state.borrow().capture.suppression().is_none());
        assert!(cx.update(|window, _| window.captured_pointer().is_none()));
        assert_eq!(
            phases.borrow().as_slice(),
            [
                OpenGpuiSceneGesturePhase::Start,
                OpenGpuiSceneGesturePhase::Cancel(
                    OpenGpuiSceneGestureCancelReason::PointerCaptureLost
                ),
            ]
        );
    }

    fn start_and_escape_successor_listener(
        handle: OpenGpuiNativeInteractionHandle,
        position: Point<Pixels>,
    ) -> OpenGpuiSceneGestureListener {
        Rc::new(move |_, window, cx| {
            let start = MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            };
            assert!(handle.begin(
                OpenGpuiSceneGestureKind::Selection,
                MouseButton::Left,
                Rc::new(|_, _, _| {}),
                None,
                &start,
                window,
                cx,
            ));
            assert!(handle.handle_key_down(
                &KeyDownEvent {
                    keystroke: open_gpui::Keystroke::parse("escape").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            ));
        })
    }

    impl CaptureGeneration for TestActive {
        fn generation(&self) -> u64 {
            self.generation
        }
    }

    #[test]
    fn stale_revocation_cannot_take_a_new_generation_and_exhaustion_is_terminal() {
        let mut capture = CaptureBookkeeping::<TestActive, (), bool>::new(false);
        capture.next_generation = Some(u64::MAX);

        assert_eq!(capture.allocate_generation(), Some(u64::MAX));
        assert_eq!(capture.allocate_generation(), None);

        capture.activate(TestActive {
            generation: u64::MAX,
        });
        assert!(capture.take_active_for_generation(u64::MAX - 1).is_none());
        assert!(capture.requires_cleanup());
        assert_eq!(
            capture.take_active_for_generation(u64::MAX),
            Some(TestActive {
                generation: u64::MAX,
            })
        );
        assert!(!capture.requires_cleanup());
    }

    #[test]
    fn pending_cancellations_stay_fifo_and_preserve_release_suppression() {
        fn assert_invariants<Suppression>(idle: Suppression, suppressed: Suppression)
        where
            Suppression: Copy + Debug + Eq,
        {
            let mut capture = CaptureBookkeeping::<TestActive, &str, Suppression>::new(idle);
            capture.enqueue("first", suppressed);
            capture.enqueue("second", suppressed);

            assert!(capture.requires_cleanup());
            assert_eq!(capture.suppression(), suppressed);
            assert!(capture.prepare_pending_drain());
            assert!(!capture.prepare_pending_drain());
            assert_eq!(
                capture.take_pending().into_iter().collect::<Vec<_>>(),
                ["first", "second"]
            );
            assert!(!capture.requires_cleanup());
            assert!(!capture.prepare_pending_drain());
            assert_eq!(capture.suppression(), suppressed);

            capture.set_suppression(idle);
            assert_eq!(capture.suppression(), idle);
        }

        assert_invariants(false, true);
        assert_invariants(None, Some(MouseButton::Middle));
    }

    #[open_gpui::test]
    fn pending_cancellation_blocks_a_new_capture_until_the_terminal_callback_drains(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let terminal_callbacks = Rc::new(Cell::new(0_u32));
        let terminal_listener: OpenGpuiSceneGestureListener = {
            let callbacks = terminal_callbacks.clone();
            Rc::new(move |_, _, _| callbacks.set(callbacks.get().saturating_add(1)))
        };
        let live_starts = Rc::new(Cell::new(0_u32));
        let live_releases = Rc::new(Cell::new(0_u32));
        let live_listener: OpenGpuiSceneGestureListener = {
            let starts = live_starts.clone();
            let releases = live_releases.clone();
            Rc::new(move |event, _, _| match event.phase() {
                OpenGpuiSceneGesturePhase::Start => {
                    starts.set(starts.get().saturating_add(1));
                }
                OpenGpuiSceneGesturePhase::Release => {
                    releases.set(releases.get().saturating_add(1));
                }
                _ => {}
            })
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let (capture_handle, _) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let mouse_down = MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        };

        let terminal_generation = handle.allocate_generation().expect("G1 generation");
        handle.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Scene(PendingSceneCancellation {
                generation: terminal_generation,
                handle: capture_handle,
                button: MouseButton::Left,
                event: OpenGpuiSceneGestureEvent {
                    kind: OpenGpuiSceneGestureKind::Selection,
                    phase: OpenGpuiSceneGesturePhase::Cancel(
                        OpenGpuiSceneGestureCancelReason::OwnerRetired,
                    ),
                    position,
                    modifiers: Default::default(),
                },
                listener: terminal_listener,
                previous_focus: None,
            }),
            Some(NativeReleaseSuppression::new(
                MouseButton::Left,
                terminal_generation,
                capture_handle,
            )),
        );

        cx.update(|window, cx| {
            window
                .capture_pointer(&capture_handle, MouseButton::Left)
                .unwrap();
            assert!(handle.begin(
                OpenGpuiSceneGestureKind::Selection,
                MouseButton::Left,
                live_listener.clone(),
                None,
                &mouse_down,
                window,
                cx,
            ));
            assert_eq!(live_starts.get(), 0);
            assert!(handle.state.borrow().capture.active().is_none());
            assert_eq!(
                handle.state.borrow().capture.latest_generation(),
                terminal_generation,
            );
            assert!(capture_handle.is_captured(window));

            assert!(handle.drain_pending(window, cx));
            assert_eq!(terminal_callbacks.get(), 1);
            assert!(window.captured_pointer().is_none());

            assert!(handle.begin(
                OpenGpuiSceneGestureKind::Selection,
                MouseButton::Left,
                live_listener,
                None,
                &mouse_down,
                window,
                cx,
            ));
            let live_generation = terminal_generation + 1;
            assert!(matches!(
                handle.state.borrow().capture.active(),
                Some(NativeCapture::Scene(active)) if active.generation == live_generation
            ));
            assert_eq!(live_starts.get(), 1);
            assert!(capture_handle.is_captured(window));
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            assert!(window.captured_pointer().is_none());
        });

        assert_eq!(terminal_callbacks.get(), 1);
        assert_eq!(live_releases.get(), 1);
        assert!(handle.state.borrow().capture.active().is_none());
    }

    #[open_gpui::test]
    fn owned_release_suppression_consumes_only_the_residual_button(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let (owner, _) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let generation = handle.allocate_generation().expect("native generation");
        let suppression = NativeReleaseSuppression::new(MouseButton::Left, generation, owner);
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));

        cx.update(|window, cx| {
            handle
                .state
                .borrow_mut()
                .capture
                .set_suppression(Some(suppression));
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            assert!(window.captured_pointer().is_none());

            handle
                .state
                .borrow_mut()
                .capture
                .set_suppression(Some(suppression));
            assert!(!handle.handle_mouse_down(
                &MouseDownEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                },
                window,
                cx,
            ));
            assert!(window.captured_pointer().is_none());
        });
    }

    #[open_gpui::test]
    fn release_suppression_does_not_own_a_followup_escape(cx: &mut open_gpui::TestAppContext) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let cancellations = Rc::new(Cell::new(0_u32));
        let listener: OpenGpuiSceneGestureListener = {
            let cancellations = cancellations.clone();
            Rc::new(move |event, _, _| {
                assert_eq!(
                    event.phase(),
                    OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::Escape),
                );
                cancellations.set(cancellations.get().saturating_add(1));
            })
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let (capture_handle, _) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let generation = handle.allocate_generation().expect("native generation");
        handle
            .state
            .borrow_mut()
            .capture
            .activate(NativeCapture::Scene(ActiveSceneInteraction {
                generation,
                kind: OpenGpuiSceneGestureKind::Selection,
                button: MouseButton::Left,
                listener,
                lease: None,
                handle: capture_handle,
                previous_focus: None,
                position,
                modifiers: Default::default(),
            }));
        let escape = KeyDownEvent {
            keystroke: open_gpui::Keystroke::parse("escape").expect("valid keystroke"),
            is_held: false,
            prefer_character_input: false,
        };

        cx.update(|window, cx| {
            window
                .capture_pointer(&capture_handle, MouseButton::Left)
                .unwrap();
            assert!(handle.handle_key_down(&escape, window, cx));
            assert_eq!(cancellations.get(), 1);
            assert!(handle.state.borrow().capture.suppression().is_some());

            assert!(
                !handle.handle_key_down(&escape, window, cx),
                "release suppression must not claim the overlay's next Escape",
            );
            assert!(handle.state.borrow().capture.suppression().is_some());
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            assert!(handle.state.borrow().capture.suppression().is_none());
        });
    }

    #[open_gpui::test]
    fn pending_suppression_does_not_swallow_a_replacement_owners_mouse_up(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let callbacks = Rc::new(Cell::new(0_u32));
        let listener: OpenGpuiSceneGestureListener = {
            let callbacks = callbacks.clone();
            Rc::new(move |_, _, _| callbacks.set(callbacks.get().saturating_add(1)))
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let (owner, successor) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let generation = handle.allocate_generation().expect("native generation");
        handle.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Scene(PendingSceneCancellation {
                generation,
                handle: owner,
                button: MouseButton::Left,
                event: OpenGpuiSceneGestureEvent {
                    kind: OpenGpuiSceneGestureKind::Selection,
                    phase: OpenGpuiSceneGesturePhase::Cancel(
                        OpenGpuiSceneGestureCancelReason::InteractionUnavailable,
                    ),
                    position,
                    modifiers: Default::default(),
                },
                listener,
                previous_focus: None,
            }),
            Some(NativeReleaseSuppression::new(
                MouseButton::Left,
                generation,
                owner,
            )),
        );
        assert_ne!(owner, successor);

        cx.update(|window, cx| {
            window
                .capture_pointer(&successor, MouseButton::Left)
                .unwrap();
            assert!(!handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            assert_eq!(callbacks.get(), 1);
            assert!(successor.is_captured(window));
            assert!(handle.state.borrow().capture.suppression().is_none());
            assert!(window.release_pointer(&successor).unwrap());
        });
    }

    #[open_gpui::test]
    fn non_escape_key_invalidates_a_replaced_release_suppression(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let (owner, successor) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let generation = handle.allocate_generation().expect("native generation");
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        handle
            .state
            .borrow_mut()
            .capture
            .set_suppression(Some(NativeReleaseSuppression::new(
                MouseButton::Left,
                generation,
                owner,
            )));

        cx.update(|window, cx| {
            window
                .capture_pointer(&successor, MouseButton::Left)
                .unwrap();
            assert!(!handle.handle_key_down(
                &KeyDownEvent {
                    keystroke: open_gpui::Keystroke::parse("a").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            ));
            assert!(handle.state.borrow().capture.suppression().is_none());

            assert!(window.release_pointer(&successor).unwrap());
            assert!(!handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
        });
    }

    #[open_gpui::test]
    fn older_terminal_stack_cannot_replace_a_successor_generations_suppression(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let (capture_handle, _) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));

        let install_outer = |listener| {
            let generation = handle.allocate_generation().expect("outer generation");
            handle
                .state
                .borrow_mut()
                .capture
                .activate(NativeCapture::Scene(ActiveSceneInteraction {
                    generation,
                    kind: OpenGpuiSceneGestureKind::Selection,
                    button: MouseButton::Left,
                    listener,
                    lease: None,
                    handle: capture_handle,
                    previous_focus: None,
                    position,
                    modifiers: Default::default(),
                }));
            generation
        };

        let release_generation = install_outer(start_and_escape_successor_listener(
            handle.clone(),
            position,
        ));
        cx.update(|window, cx| {
            window
                .capture_pointer(&capture_handle, MouseButton::Left)
                .unwrap();
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            let suppression = handle
                .state
                .borrow()
                .capture
                .suppression()
                .expect("successor suppression survives outer release");
            assert_eq!(suppression.generation, release_generation + 1);
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
        });

        let escape_generation = install_outer(start_and_escape_successor_listener(
            handle.clone(),
            position,
        ));
        cx.update(|window, cx| {
            window
                .capture_pointer(&capture_handle, MouseButton::Left)
                .unwrap();
            assert!(handle.handle_key_down(
                &KeyDownEvent {
                    keystroke: open_gpui::Keystroke::parse("escape").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            ));
            let suppression = handle
                .state
                .borrow()
                .capture
                .suppression()
                .expect("successor suppression survives outer escape");
            assert_eq!(suppression.generation, escape_generation + 1);
            assert!(handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
        });
    }

    #[open_gpui::test]
    fn capture_loss_does_not_release_or_swallow_replacement_events(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let listener: OpenGpuiSceneGestureListener = {
            let phases = phases.clone();
            Rc::new(move |event, _, _| phases.borrow_mut().push(event.phase()))
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let (owner, successor) = draw_capture_handles(&view, cx, &handle, MouseButton::Left);
        assert_ne!(owner, successor);
        let install_capture = |capture_handle| {
            let generation = handle.allocate_generation().expect("native generation");
            handle
                .state
                .borrow_mut()
                .capture
                .activate(NativeCapture::Scene(ActiveSceneInteraction {
                    generation,
                    kind: OpenGpuiSceneGestureKind::Selection,
                    button: MouseButton::Left,
                    listener: listener.clone(),
                    lease: None,
                    handle: capture_handle,
                    previous_focus: None,
                    position,
                    modifiers: Default::default(),
                }));
        };

        install_capture(owner);
        cx.update(|window, cx| {
            window
                .capture_pointer(&successor, MouseButton::Left)
                .unwrap();
            assert!(!handle.handle_mouse_up(
                &MouseUpEvent {
                    position,
                    modifiers: Default::default(),
                    button: MouseButton::Left,
                    click_count: 1,
                },
                window,
                cx,
            ));
            assert!(successor.is_captured(window));
            assert!(handle.drain_pending(window, cx));
            assert!(successor.is_captured(window));
            assert!(window.release_pointer(&successor).unwrap());
        });

        install_capture(owner);
        cx.update(|window, cx| {
            window
                .capture_pointer(&successor, MouseButton::Left)
                .unwrap();
            assert!(!handle.handle_key_down(
                &KeyDownEvent {
                    keystroke: open_gpui::Keystroke::parse("escape").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            ));
            assert!(successor.is_captured(window));
            assert!(handle.drain_pending(window, cx));
            assert!(successor.is_captured(window));
            assert!(window.release_pointer(&successor).unwrap());
        });

        assert_eq!(
            phases.borrow().as_slice(),
            [
                OpenGpuiSceneGesturePhase::Cancel(
                    OpenGpuiSceneGestureCancelReason::PointerCaptureLost,
                ),
                OpenGpuiSceneGesturePhase::Cancel(
                    OpenGpuiSceneGestureCancelReason::PointerCaptureLost,
                ),
            ]
        );
    }

    #[open_gpui::test]
    fn terminal_native_cancellation_dispatches_once_and_repeated_close_is_inert(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        let authority = OpenGpuiNativeInteractionAuthority::new();
        let handle = authority.capability();
        let callbacks = Rc::new(Cell::new(0_u32));
        let listener: OpenGpuiSceneGestureListener = {
            let callbacks = callbacks.clone();
            Rc::new(move |event, _, _| {
                assert_eq!(
                    event.phase(),
                    OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::HostClosed)
                );
                callbacks.set(callbacks.get().saturating_add(1));
            })
        };
        let generation = handle.allocate_generation().expect("native generation");
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let (capture_handle, successor) =
            draw_capture_handles(&view, cx, &handle, MouseButton::Middle);
        handle
            .state
            .borrow_mut()
            .capture
            .activate(NativeCapture::Scene(ActiveSceneInteraction {
                generation,
                kind: OpenGpuiSceneGestureKind::Pan,
                button: MouseButton::Middle,
                listener,
                lease: None,
                handle: capture_handle,
                previous_focus: None,
                position,
                modifiers: Default::default(),
            }));

        cx.update(|window, cx| {
            window
                .capture_pointer(&capture_handle, MouseButton::Middle)
                .unwrap();
            assert!(authority.cancel_for_host_close(window, cx));
            assert!(window.captured_pointer().is_none());

            window
                .capture_pointer(&successor, MouseButton::Middle)
                .unwrap();
            assert!(!authority.cancel_for_host_close(window, cx));
            assert!(
                successor.is_captured(window),
                "repeated close must not release a later owner's pointer capture",
            );
        });
        cx.run_until_parked();

        assert_eq!(callbacks.get(), 1);
        assert!(handle.state.borrow().capture.active().is_none());
        cx.update(|window, _| window.release_pointer(&successor).unwrap());
    }

    #[open_gpui::test]
    fn terminal_restore_returns_to_the_previous_focus(cx: &mut open_gpui::TestAppContext) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            bind_focus_handles(&view, &[previous.clone(), interaction.clone()], window, cx);
            focus_presented(&interaction, window, cx);
            assert_eq!(window.focused(cx), Some(interaction));

            settle_native_focus(ownership, &[], OpenGpuiPortFocusExit::Restore, window, cx);
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(previous));
        });
    }

    #[open_gpui::test]
    fn terminal_restore_does_not_steal_a_new_live_focus(cx: &mut open_gpui::TestAppContext) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let unrelated = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            bind_focus_handles(
                &view,
                &[previous.clone(), interaction.clone(), unrelated.clone()],
                window,
                cx,
            );
            focus_presented(&interaction, window, cx);
            focus_presented(&unrelated, window, cx);
            assert_eq!(window.focused(cx), Some(unrelated.clone()));

            settle_native_focus(ownership, &[], OpenGpuiPortFocusExit::Restore, window, cx);
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(unrelated));
        });
    }

    #[open_gpui::test]
    fn search_overlay_restore_returns_to_the_initiating_focus(cx: &mut open_gpui::TestAppContext) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let search_overlay = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            bind_focus_handles(
                &view,
                &[
                    previous.clone(),
                    interaction.clone(),
                    search_overlay.clone(),
                ],
                window,
                cx,
            );
            focus_presented(&search_overlay, window, cx);
            assert_eq!(window.focused(cx), Some(search_overlay.clone()));

            settle_native_focus(
                ownership,
                &[search_overlay.downgrade()],
                OpenGpuiPortFocusExit::RestoreSearchOverlay,
                window,
                cx,
            );
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(previous));
        });
    }

    #[open_gpui::test]
    fn search_overlay_restore_does_not_steal_unregistered_focus(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let search_overlay = cx.focus_handle();
            let unrelated = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            bind_focus_handles(
                &view,
                &[
                    previous.clone(),
                    interaction.clone(),
                    search_overlay.clone(),
                    unrelated.clone(),
                ],
                window,
                cx,
            );
            focus_presented(&unrelated, window, cx);
            assert_eq!(window.focused(cx), Some(unrelated.clone()));

            settle_native_focus(
                ownership,
                &[search_overlay.downgrade()],
                OpenGpuiPortFocusExit::RestoreSearchOverlay,
                window,
                cx,
            );
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(unrelated));
        });
    }

    #[open_gpui::test]
    fn terminal_prepare_retains_registered_overlay_focus_for_window_cleanup(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let authority = OpenGpuiNativeInteractionAuthority::new();
        let handle = authority.capability();
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let overlay = cx.focus_handle();
            let unrelated = cx.focus_handle();
            bind_focus_handles(&view, &[overlay.clone(), unrelated.clone()], window, cx);
            handle.mark_overlay_focus_domain(&overlay);
            focus_presented(&overlay, window, cx);
            assert_eq!(window.focused(cx), Some(overlay));
            assert!(handle.requires_window_cleanup());

            authority.prepare_host_close_deferred();
            assert!(authority.cancel_for_host_close(window, cx));
            window.draw(cx).clear(cx);
            assert!(window.focused(cx).is_none());

            focus_presented(&unrelated, window, cx);
            assert_eq!(window.focused(cx), Some(unrelated.clone()));
            assert!(!authority.cancel_for_host_close(window, cx));
            assert_eq!(window.focused(cx), Some(unrelated));
        });
    }

    #[open_gpui::test]
    fn terminal_restore_falls_back_to_the_canvas_when_previous_focus_is_gone(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let interaction = cx.focus_handle();
            let previous = cx.focus_handle().downgrade();
            let ownership = NativeFocusOwnership {
                previous_focus: Some(previous),
                previous_focus_was_graph_domain: false,
                interaction_focus: Some(interaction.downgrade()),
            };
            bind_focus_handles(&view, std::slice::from_ref(&interaction), window, cx);
            assert!(
                ownership
                    .previous_focus
                    .as_ref()
                    .and_then(WeakFocusHandle::upgrade)
                    .is_none()
            );
            window.blur(cx);

            settle_native_focus(ownership, &[], OpenGpuiPortFocusExit::Restore, window, cx);
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(interaction));
        });
    }

    #[open_gpui::test]
    fn terminal_restore_falls_back_to_canvas_when_previous_graph_focus_is_detached(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let ownership = NativeFocusOwnership {
                previous_focus: Some(previous.downgrade()),
                previous_focus_was_graph_domain: true,
                interaction_focus: Some(interaction.downgrade()),
            };
            bind_focus_handles(&view, &[previous.clone(), interaction.clone()], window, cx);
            focus_presented(&previous, window, cx);
            assert_eq!(window.focused(cx), Some(previous.clone()));
            bind_focus_handles(&view, std::slice::from_ref(&interaction), window, cx);
            assert!(!window.is_focus_handle_rendered(&previous));
            assert!(window.focused(cx).is_none());
            focus_presented(&interaction, window, cx);
            assert_eq!(window.focused(cx), Some(interaction.clone()));

            settle_native_focus(ownership, &[], OpenGpuiPortFocusExit::Restore, window, cx);
            window.draw(cx).clear(cx);

            assert_eq!(window.focused(cx), Some(interaction));
        });
    }

    #[open_gpui::test]
    fn retirement_and_host_close_only_blur_owned_interaction_focus(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (view, cx) = focus_test_window(cx);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let unrelated = cx.focus_handle();
            bind_focus_handles(
                &view,
                &[previous.clone(), interaction.clone(), unrelated.clone()],
                window,
                cx,
            );
            focus_presented(&interaction, window, cx);
            assert_eq!(window.focused(cx), Some(interaction.clone()));
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                &[],
                OpenGpuiPortFocusExit::OwnerRetired,
                window,
                cx,
            );
            window.draw(cx).clear(cx);
            assert!(window.focused(cx).is_none());

            focus_presented(&unrelated, window, cx);
            assert_eq!(window.focused(cx), Some(unrelated.clone()));
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                &[],
                OpenGpuiPortFocusExit::OwnerRetired,
                window,
                cx,
            );
            window.draw(cx).clear(cx);
            assert_eq!(window.focused(cx), Some(unrelated.clone()));

            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                &[],
                OpenGpuiPortFocusExit::HostClosed,
                window,
                cx,
            );
            window.draw(cx).clear(cx);
            assert_eq!(window.focused(cx), Some(unrelated));

            focus_presented(&interaction, window, cx);
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                &[],
                OpenGpuiPortFocusExit::HostClosed,
                window,
                cx,
            );
            window.draw(cx).clear(cx);
            assert!(window.focused(cx).is_none());
        });
    }

    fn native_focus_ownership(
        previous: &FocusHandle,
        interaction: &FocusHandle,
    ) -> NativeFocusOwnership {
        NativeFocusOwnership {
            previous_focus: Some(previous.downgrade()),
            previous_focus_was_graph_domain: false,
            interaction_focus: Some(interaction.downgrade()),
        }
    }
}
