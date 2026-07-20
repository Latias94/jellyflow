use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem;
use std::rc::{Rc, Weak};

use jellyflow::core::{EdgeId, PortId};
use jellyflow::runtime::{EdgeEndpoint, NodeInstanceKey};
use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, GlobalElementId,
    Hitbox, HitboxBehavior, HitboxId, InspectorElementId, InteractiveElement, IntoElement,
    KeyDownEvent, LayoutId, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, PinchEvent, Pixels, Point, ScrollWheelEvent, Styled, WeakFocusHandle, Window,
    div,
};

use crate::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
use crate::binding::OpenGpuiMountCapability;

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

/// Graph-semantic lifecycle phase emitted by [`crate::OpenGpuiPortHandle::interactive`].
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
    port: PortId,
}

impl OpenGpuiPortDragOwner {
    #[must_use]
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    #[must_use]
    pub fn port(&self) -> PortId {
        self.port
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
    hitbox: HitboxId,
}

impl NativeReleaseSuppression {
    fn new(button: MouseButton, generation: u64, hitbox: HitboxId) -> Self {
        Self {
            button,
            generation,
            hitbox,
        }
    }
}

type NativeCaptureBookkeeping =
    CaptureBookkeeping<NativeCapture, PendingNativeCancellation, Option<NativeReleaseSuppression>>;

struct NativeInteractionState {
    capture: NativeCaptureBookkeeping,
    root_hitbox: Option<HitboxId>,
    focus_handle: Option<FocusHandle>,
    port_focus: Option<PortFocusOwnership>,
    overlay_active: bool,
    terminal: bool,
}

enum NativeCapture {
    Port(ActivePortDrag),
    Scene(ActiveSceneInteraction),
}

enum PendingNativeCancellation {
    Port(PendingPortCancellation),
    Scene(PendingSceneCancellation),
}

impl PendingNativeCancellation {
    fn generation(&self) -> u64 {
        match self {
            Self::Port(pending) => pending.generation,
            Self::Scene(pending) => pending.generation,
        }
    }

    fn hitbox(&self) -> HitboxId {
        match self {
            Self::Port(pending) => pending.hitbox,
            Self::Scene(pending) => pending.hitbox,
        }
    }
}

struct ActivePortDrag {
    generation: u64,
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    listener: OpenGpuiPortListener,
    lease: OpenGpuiActivityLease,
    hitbox: HitboxId,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

struct PendingPortCancellation {
    generation: u64,
    hitbox: HitboxId,
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
    interaction_focus: Option<WeakFocusHandle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiPortFocusExit {
    Restore,
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
                root_hitbox: None,
                focus_handle: None,
                port_focus: None,
                overlay_active: false,
                terminal: false,
            })),
        }
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
        }
    }

    fn mark_overlay_active(&self) {
        let mut state = self.state.borrow_mut();
        if !state.terminal {
            state.overlay_active = true;
        }
    }

    fn overlay_active(&self) -> bool {
        self.state.borrow().overlay_active
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
        state.capture.requires_cleanup() || state.port_focus.is_some()
    }

    pub(crate) fn settle_port_focus(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        exit: OpenGpuiPortFocusExit,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let ownership = {
            let mut state = self.state.borrow_mut();
            state
                .port_focus
                .take_if(|current| current.owner == *owner && current.mount.is_same_mount(mount))
        };
        let Some(ownership) = ownership else {
            return false;
        };
        settle_native_focus(ownership.focus, exit, window, cx);
        true
    }

    pub(crate) fn rebind_port(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        hitbox: HitboxId,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !mount.is_live() {
            return;
        }
        let captured_hitbox = window.captured_hitbox();
        let lost = {
            let mut state = self.state.borrow_mut();
            let previous_hitbox = match state.capture.active() {
                Some(NativeCapture::Port(active))
                    if &active.owner == owner && active.mount.is_same_mount(mount) =>
                {
                    active.hitbox
                }
                _ => return,
            };
            if captured_hitbox != Some(previous_hitbox) {
                match state.capture.take_active() {
                    Some(NativeCapture::Port(active)) => Some(active),
                    _ => unreachable!("matched port capture"),
                }
            } else {
                let active_mount = match state.capture.active_mut() {
                    Some(NativeCapture::Port(active)) => {
                        active.hitbox = hitbox;
                        active.mount.clone()
                    }
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
            return;
        }
        window.capture_pointer(hitbox);
    }

    pub(crate) fn begin_port_drag(
        &self,
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        listener: OpenGpuiPortListener,
        activity: OpenGpuiNodeActivityRegistry,
        hitbox: HitboxId,
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
            activity.acquire_with_revocation(OpenGpuiActivityKind::PointerCapture, move || {
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

        let previous_focus = window.focused(cx).map(|focus| focus.downgrade());
        let interaction_focus = self.state.borrow().focus_handle.clone();
        if let Some(focus_handle) = &interaction_focus {
            window.focus(focus_handle, cx);
        }
        window.capture_pointer(hitbox);

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
                    interaction_focus: interaction_focus.map(|focus| focus.downgrade()),
                },
            });
            state.capture.activate(NativeCapture::Port(ActivePortDrag {
                generation,
                owner,
                mount,
                listener: listener.clone(),
                lease,
                hitbox,
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
                self.finish_native_capture(
                    active,
                    NativeCaptureExit::Release,
                    event.position,
                    event.modifiers,
                    window,
                    cx,
                );
            }
            self.state.borrow_mut().capture.set_suppression(None);
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
        _event: &ScrollWheelEvent,
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
        if !event.keystroke.key.eq_ignore_ascii_case("escape") {
            return false;
        }
        if self.cancel_replaced_capture_deferred(window, cx) {
            return false;
        }
        self.drain_pending(window, cx);
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            let position = active.position();
            let modifiers = active.modifiers();
            let suppression = active.release_suppression();
            self.finish_native_capture(
                active,
                NativeCaptureExit::Escape,
                position,
                modifiers,
                window,
                cx,
            );
            self.state
                .borrow_mut()
                .capture
                .set_suppression(Some(suppression));
            return true;
        }
        self.release_suppression_for_window(window).is_some()
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
            let PendingNativeCancellation::Port(PendingPortCancellation {
                generation: _,
                hitbox: _,
                event,
                listener,
                focus_settlement,
            }) = pending
            else {
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
        let hitbox = active.hitbox;
        let event = OpenGpuiPortDragEvent {
            owner: active.owner,
            native_generation: Some(NativeInteractionGeneration(active.generation)),
            phase,
            position,
            modifiers,
        };
        let listener = active.listener;
        drop(active.lease);
        release_pointer_capture_if_owned(hitbox, window);
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
            NativeReleaseSuppression::new(MouseButton::Left, active.generation, active.hitbox)
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
            PendingNativeCancellation::Port(PendingPortCancellation {
                generation: active.generation,
                hitbox: active.hitbox,
                event,
                listener: active.listener,
                focus_settlement,
            }),
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
    EdgeReconnect(EdgeId, EdgeEndpoint),
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
    hitbox: HitboxId,
    previous_focus: Option<WeakFocusHandle>,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

struct PendingSceneCancellation {
    generation: u64,
    hitbox: HitboxId,
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

    fn hitbox(&self) -> HitboxId {
        match self {
            Self::Port(active) => active.hitbox,
            Self::Scene(active) => active.hitbox,
        }
    }

    fn release_suppression(&self) -> NativeReleaseSuppression {
        NativeReleaseSuppression::new(self.button(), self.generation(), self.hitbox())
    }
}

impl OpenGpuiNativeInteractionHandle {
    pub(crate) fn set_root_hitbox(&self, hitbox: HitboxId, window: &mut Window, cx: &mut App) {
        let captured_hitbox = window.captured_hitbox();
        let (rebound, lost) = {
            let mut state = self.state.borrow_mut();
            if state.terminal {
                return;
            }
            state.root_hitbox = Some(hitbox);
            let previous_hitbox = match state.capture.active() {
                Some(NativeCapture::Scene(active)) => active.hitbox,
                _ => return,
            };
            if captured_hitbox == Some(previous_hitbox) {
                match state.capture.active_mut() {
                    Some(NativeCapture::Scene(active)) => active.hitbox = hitbox,
                    _ => unreachable!("matched scene capture"),
                }
                (true, None)
            } else {
                let lost = match state.capture.take_active() {
                    Some(NativeCapture::Scene(active)) => active,
                    _ => unreachable!("matched scene capture"),
                };
                (false, Some(lost))
            }
        };
        if let Some(active) = lost {
            self.queue_scene_cancellation_without_suppression(
                active,
                OpenGpuiSceneGestureCancelReason::PointerCaptureLost,
            );
            self.schedule_pending_drain(window, cx);
            return;
        }
        if rebound {
            window.capture_pointer(hitbox);
        }
    }

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
                .acquire_with_revocation(OpenGpuiActivityKind::PointerCapture, move || {
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
        let Some(hitbox) = self.state.borrow().root_hitbox else {
            drop(lease);
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
        window.capture_pointer(hitbox);
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
                hitbox,
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
            state.root_hitbox = None;
            state.focus_handle = None;
            state.port_focus = None;
            state.overlay_active = false;
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
        let pending = self.state.borrow_mut().capture.take_pending();
        if !prepared && pending.is_empty() {
            return false;
        }
        release_matching_pointer_capture(&pending, window);
        window.refresh();
        if pending.is_empty() {
            return prepared;
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
    let owns_capture = window
        .captured_hitbox()
        .is_some_and(|captured| pending.iter().any(|pending| pending.hitbox() == captured));
    if owns_capture {
        window.release_pointer();
    }
    owns_capture
}

fn release_pointer_capture_if_owned(hitbox: HitboxId, window: &mut Window) -> bool {
    if window.captured_hitbox() != Some(hitbox) {
        return false;
    }
    window.release_pointer();
    true
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
        let hitbox = active.hitbox;
        let event = OpenGpuiSceneGestureEvent {
            kind: active.kind,
            phase,
            position,
            modifiers,
        };
        let previous_focus = active.previous_focus;
        let interaction_focus = self.state.borrow().focus_handle.clone();
        drop(active.lease);
        release_pointer_capture_if_owned(hitbox, window);
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
            NativeReleaseSuppression::new(active.button, active.generation, active.hitbox)
        });
        let previous_focus = active.previous_focus;
        drop(active.lease);
        self.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Scene(PendingSceneCancellation {
                generation: active.generation,
                hitbox: active.hitbox,
                event,
                listener: active.listener,
                previous_focus,
            }),
            suppression,
        );
    }

    fn cancel_replaced_capture_deferred(&self, window: &Window, cx: &mut App) -> bool {
        let captured_hitbox = window.captured_hitbox();
        let active = {
            let mut state = self.state.borrow_mut();
            if state
                .capture
                .active()
                .is_none_or(|active| captured_hitbox == Some(active.hitbox()))
            {
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
        let capture_is_unclaimed_or_owned = window
            .captured_hitbox()
            .is_none_or(|captured| captured == suppression.hitbox);
        if is_current_generation && capture_is_unclaimed_or_owned {
            return Some(suppression);
        }
        state.capture.set_suppression(None);
        None
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
                NativeReleaseSuppression::new(MouseButton::Left, active.generation, active.hitbox);
            let event = OpenGpuiPortDragEvent {
                owner: active.owner,
                native_generation: Some(NativeInteractionGeneration(active.generation)),
                phase: OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
                position: active.position,
                modifiers: active.modifiers,
            };
            drop(active.lease);
            state.borrow_mut().capture.enqueue(
                PendingNativeCancellation::Port(PendingPortCancellation {
                    generation: active.generation,
                    hitbox: active.hitbox,
                    event,
                    listener: active.listener,
                    focus_settlement: None,
                }),
                Some(suppression),
            );
        }
        NativeCapture::Scene(active) => {
            let suppression =
                NativeReleaseSuppression::new(active.button, active.generation, active.hitbox);
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
                    hitbox: active.hitbox,
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
    focus_handle: FocusHandle,
    graph_key_listener: Option<OpenGpuiGraphKeyListener>,
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
        focus_handle,
        graph_key_listener,
        child: Some(child.into_any_element()),
    }
}

pub(crate) type OpenGpuiGraphKeyListener =
    Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> bool + 'static>;

struct OpenGpuiInteractionRoot {
    id: ElementId,
    interactions: OpenGpuiNativeInteractionHandle,
    focus_handle: FocusHandle,
    graph_key_listener: Option<OpenGpuiGraphKeyListener>,
    child: Option<AnyElement>,
}

impl Element for OpenGpuiInteractionRoot {
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
        let mut child = self.child.take().expect("interaction root child missing");
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
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.interactions.schedule_pending_drain(window, cx);
        self.interactions.set_root_hitbox(hitbox.id, window, cx);
        window.set_focus_handle(&self.focus_handle, cx);
        child.prepaint(window, cx);
        hitbox
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
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
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
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && interactions.handle_key_down(event, window, cx) {
                cx.stop_propagation();
                return;
            }
            if phase == DispatchPhase::Bubble
                && !window.default_prevented()
                && is_unmodified_tab(event)
            {
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                window.prevent_default();
                cx.stop_propagation();
            }
        });
        if let Some(listener) = self.graph_key_listener.clone() {
            let interactions = self.interactions.clone();
            window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && !window.default_prevented()
                    && !interactions.overlay_active()
                    && listener(event, window, cx)
                {
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
    child: Option<AnyElement>,
}

impl OpenGpuiOverlayInteractionElement {
    fn new(interactions: OpenGpuiNativeInteractionHandle, child: impl IntoElement) -> Self {
        Self {
            interactions,
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
        window.on_mouse_event(move |_: &ScrollWheelEvent, phase, window, cx| {
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

    match exit {
        OpenGpuiPortFocusExit::Restore => {
            let current_is_previous = current_focus
                .as_ref()
                .zip(previous_focus.as_ref())
                .is_some_and(|(current, previous)| current == previous);
            if current_is_previous {
                return;
            }
            let current_is_interaction = current_focus
                .as_ref()
                .zip(interaction_focus.as_ref())
                .is_some_and(|(current, interaction)| current == interaction);
            if current_focus.is_some() && !current_is_interaction {
                return;
            }
            if let Some(previous_focus) = previous_focus {
                window.focus(&previous_focus, cx);
            } else if let Some(interaction_focus) = interaction_focus {
                window.focus(&interaction_focus, cx);
            } else {
                window.blur();
            }
        }
        OpenGpuiPortFocusExit::OwnerRetired => {
            let current_is_interaction = current_focus
                .as_ref()
                .zip(interaction_focus.as_ref())
                .is_some_and(|(current, interaction)| current == interaction);
            if current_is_interaction {
                window.blur();
            }
        }
        OpenGpuiPortFocusExit::HostClosed => {
            if interaction_focus
                .as_ref()
                .is_some_and(|interaction| interaction.contains_focused(window, cx))
            {
                window.blur();
            }
        }
    }
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
        window.blur();
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

pub(crate) fn port_drag_owner(instance: NodeInstanceKey, port: PortId) -> OpenGpuiPortDragOwner {
    OpenGpuiPortDragOwner { instance, port }
}

#[cfg(test)]
mod capture_bookkeeping_tests {
    use std::cell::Cell;
    use std::fmt::Debug;

    use open_gpui::{Context, Render};

    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct TestActive {
        generation: u64,
    }

    struct FocusTestView;

    impl Render for FocusTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            open_gpui::div()
        }
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
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
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
        let hitbox = HitboxId::placeholder();
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
                hitbox,
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
                hitbox,
            )),
        );

        cx.update(|window, cx| {
            handle.set_root_hitbox(hitbox, window, cx);
            window.capture_pointer(hitbox);
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
            assert_eq!(window.captured_hitbox(), Some(hitbox));

            assert!(handle.drain_pending(window, cx));
            assert_eq!(terminal_callbacks.get(), 1);
            assert!(window.captured_hitbox().is_none());

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
            assert_eq!(window.captured_hitbox(), Some(hitbox));
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
            assert!(window.captured_hitbox().is_none());
        });

        assert_eq!(terminal_callbacks.get(), 1);
        assert_eq!(live_releases.get(), 1);
        assert!(handle.state.borrow().capture.active().is_none());
    }

    #[open_gpui::test]
    fn owned_release_suppression_consumes_only_the_residual_button(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let owner = HitboxId::placeholder();
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
            assert!(window.captured_hitbox().is_none());

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
            assert!(window.captured_hitbox().is_none());
        });
    }

    #[open_gpui::test]
    fn pending_suppression_does_not_swallow_a_replacement_owners_mouse_up(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let callbacks = Rc::new(Cell::new(0_u32));
        let listener: OpenGpuiSceneGestureListener = {
            let callbacks = callbacks.clone();
            Rc::new(move |_, _, _| callbacks.set(callbacks.get().saturating_add(1)))
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let owner = HitboxId::placeholder();
        let generation = handle.allocate_generation().expect("native generation");
        handle.state.borrow_mut().capture.enqueue(
            PendingNativeCancellation::Scene(PendingSceneCancellation {
                generation,
                hitbox: owner,
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
        let _ = cx.draw(
            open_gpui::point(open_gpui::px(0.0), open_gpui::px(0.0)),
            open_gpui::size(open_gpui::px(10.0), open_gpui::px(10.0)),
            |_, _| {
                OpenGpuiOverlayInteractionElement::new(
                    OpenGpuiNativeInteractionHandle::new(),
                    open_gpui::div().size_full(),
                )
            },
        );
        let (_, successor_hitbox) = cx.draw(
            open_gpui::point(open_gpui::px(0.0), open_gpui::px(0.0)),
            open_gpui::size(open_gpui::px(10.0), open_gpui::px(10.0)),
            |_, _| {
                OpenGpuiOverlayInteractionElement::new(
                    OpenGpuiNativeInteractionHandle::new(),
                    open_gpui::div().size_full(),
                )
            },
        );
        let successor = successor_hitbox.id;
        assert_ne!(owner, successor);

        cx.update(|window, cx| {
            window.capture_pointer(successor);
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
            assert_eq!(window.captured_hitbox(), Some(successor));
            assert!(handle.state.borrow().capture.suppression().is_none());
            window.release_pointer();
        });
    }

    #[open_gpui::test]
    fn capture_loss_does_not_release_or_swallow_replacement_events(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        let handle = OpenGpuiNativeInteractionHandle::new();
        let phases = Rc::new(RefCell::new(Vec::new()));
        let listener: OpenGpuiSceneGestureListener = {
            let phases = phases.clone();
            Rc::new(move |event, _, _| phases.borrow_mut().push(event.phase()))
        };
        let position = open_gpui::point(open_gpui::px(20.0), open_gpui::px(30.0));
        let _ = cx.draw(
            open_gpui::point(open_gpui::px(0.0), open_gpui::px(0.0)),
            open_gpui::size(open_gpui::px(10.0), open_gpui::px(10.0)),
            |_, _| {
                OpenGpuiOverlayInteractionElement::new(
                    OpenGpuiNativeInteractionHandle::new(),
                    open_gpui::div().size_full(),
                )
            },
        );
        let (_, successor_hitbox) = cx.draw(
            open_gpui::point(open_gpui::px(0.0), open_gpui::px(0.0)),
            open_gpui::size(open_gpui::px(10.0), open_gpui::px(10.0)),
            |_, _| {
                OpenGpuiOverlayInteractionElement::new(
                    OpenGpuiNativeInteractionHandle::new(),
                    open_gpui::div().size_full(),
                )
            },
        );
        let successor = successor_hitbox.id;
        let owner = HitboxId::placeholder();
        assert_ne!(owner, successor);
        let install_capture = |hitbox| {
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
                    hitbox,
                    previous_focus: None,
                    position,
                    modifiers: Default::default(),
                }));
        };

        install_capture(owner);
        cx.update(|window, cx| {
            window.capture_pointer(successor);
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
            assert_eq!(window.captured_hitbox(), Some(successor));
            assert!(handle.drain_pending(window, cx));
            assert_eq!(window.captured_hitbox(), Some(successor));
            window.release_pointer();
        });

        install_capture(owner);
        cx.update(|window, cx| {
            window.capture_pointer(successor);
            assert!(!handle.handle_key_down(
                &KeyDownEvent {
                    keystroke: open_gpui::Keystroke::parse("escape").expect("valid keystroke"),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            ));
            assert_eq!(window.captured_hitbox(), Some(successor));
            assert!(handle.drain_pending(window, cx));
            assert_eq!(window.captured_hitbox(), Some(successor));
            window.release_pointer();
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
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
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
        let hitbox = HitboxId::placeholder();
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
                hitbox,
                previous_focus: None,
                position,
                modifiers: Default::default(),
            }));

        cx.update(|window, cx| {
            window.capture_pointer(hitbox);
            assert!(authority.cancel_for_host_close(window, cx));
            assert!(window.captured_hitbox().is_none());

            window.capture_pointer(hitbox);
            assert!(!authority.cancel_for_host_close(window, cx));
            assert_eq!(
                window.captured_hitbox(),
                Some(hitbox),
                "repeated close must not release a later owner's pointer capture",
            );
        });
        cx.run_until_parked();

        assert_eq!(callbacks.get(), 1);
        assert!(handle.state.borrow().capture.active().is_none());
        cx.update(|window, _| window.release_pointer());
    }

    #[open_gpui::test]
    fn terminal_restore_returns_to_the_previous_focus(cx: &mut open_gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            window.focus(&interaction, cx);

            settle_native_focus(ownership, OpenGpuiPortFocusExit::Restore, window, cx);

            assert_eq!(window.focused(cx), Some(previous));
        });
    }

    #[open_gpui::test]
    fn terminal_restore_does_not_steal_a_new_live_focus(cx: &mut open_gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let unrelated = cx.focus_handle();
            let ownership = native_focus_ownership(&previous, &interaction);
            window.focus(&interaction, cx);
            window.focus(&unrelated, cx);

            settle_native_focus(ownership, OpenGpuiPortFocusExit::Restore, window, cx);

            assert_eq!(window.focused(cx), Some(unrelated));
        });
    }

    #[open_gpui::test]
    fn terminal_restore_falls_back_to_the_canvas_when_previous_focus_is_gone(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let interaction = cx.focus_handle();
            let previous = cx.focus_handle().downgrade();
            let ownership = NativeFocusOwnership {
                previous_focus: Some(previous),
                interaction_focus: Some(interaction.downgrade()),
            };
            window.blur();

            settle_native_focus(ownership, OpenGpuiPortFocusExit::Restore, window, cx);

            assert_eq!(window.focused(cx), Some(interaction));
        });
    }

    #[open_gpui::test]
    fn retirement_and_host_close_only_blur_owned_interaction_focus(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let previous = cx.focus_handle();
            let interaction = cx.focus_handle();
            let unrelated = cx.focus_handle();
            window.focus(&interaction, cx);
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                OpenGpuiPortFocusExit::OwnerRetired,
                window,
                cx,
            );
            assert!(window.focused(cx).is_none());

            window.focus(&unrelated, cx);
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                OpenGpuiPortFocusExit::OwnerRetired,
                window,
                cx,
            );
            assert_eq!(window.focused(cx), Some(unrelated.clone()));

            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                OpenGpuiPortFocusExit::HostClosed,
                window,
                cx,
            );
            assert_eq!(window.focused(cx), Some(unrelated));

            window.focus(&interaction, cx);
            settle_native_focus(
                native_focus_ownership(&previous, &interaction),
                OpenGpuiPortFocusExit::HostClosed,
                window,
                cx,
            );
            assert!(window.focused(cx).is_none());
        });
    }

    fn native_focus_ownership(
        previous: &FocusHandle,
        interaction: &FocusHandle,
    ) -> NativeFocusOwnership {
        NativeFocusOwnership {
            previous_focus: Some(previous.downgrade()),
            interaction_focus: Some(interaction.downgrade()),
        }
    }
}
