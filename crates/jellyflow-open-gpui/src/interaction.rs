use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem;
use std::rc::{Rc, Weak};

use jellyflow::core::PortId;
use jellyflow::runtime::NodeInstanceKey;
use open_gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, FocusHandle, GlobalElementId,
    Hitbox, HitboxBehavior, HitboxId, InspectorElementId, IntoElement, KeyDownEvent, LayoutId,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels,
    Point, ScrollWheelEvent, WeakFocusHandle, Window,
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
    phase: OpenGpuiPortDragPhase,
    position: Point<Pixels>,
    modifiers: Modifiers,
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

/// Thin event boundary for ordinary native controls, scroll regions, and overlays inside a node.
///
/// The guard does not describe or render a widget. It lets the native child handle its event first,
/// then stops unhandled pointer input before it can reach node chrome or the canvas. Overlay owners
/// must separately hold an [`OpenGpuiActivityKind::Overlay`] lease for retained-lifecycle protection.
pub struct OpenGpuiNodeInteractionGuard;

impl OpenGpuiNodeInteractionGuard {
    /// Prevents pointer input handled by a native control from falling through to graph gestures.
    pub fn control(child: impl IntoElement) -> impl IntoElement {
        OpenGpuiInteractionGuardElement::new(child, GuardMode::Embedded)
    }

    /// Prevents native scrolling and scrollbar gestures from reaching node chrome or canvas zoom.
    pub fn scroll(child: impl IntoElement) -> impl IntoElement {
        OpenGpuiInteractionGuardElement::new(child, GuardMode::Embedded)
    }

    /// Occludes graph hit targets behind a native overlay and stops its unhandled pointer input.
    ///
    /// The retained overlay entity should hold an overlay activity lease for as long as it is open.
    pub fn overlay(child: impl IntoElement) -> impl IntoElement {
        OpenGpuiInteractionGuardElement::new(child, GuardMode::Overlay)
    }
}

impl OpenGpuiPortDragEvent {
    #[must_use]
    pub fn owner(&self) -> &OpenGpuiPortDragOwner {
        &self.owner
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
pub(crate) struct OpenGpuiInteractionCoordinator {
    state: Rc<RefCell<InteractionState>>,
}

type PortCaptureBookkeeping = CaptureBookkeeping<ActivePortDrag, PendingCancellation, bool>;

struct InteractionState {
    capture: PortCaptureBookkeeping,
    focus_handle: Option<FocusHandle>,
}

struct ActivePortDrag {
    generation: u64,
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    listener: OpenGpuiPortListener,
    lease: OpenGpuiActivityLease,
    previous_focus: Option<WeakFocusHandle>,
    hitbox: HitboxId,
    position: Point<Pixels>,
    modifiers: Modifiers,
}

struct PendingCancellation {
    event: OpenGpuiPortDragEvent,
    listener: OpenGpuiPortListener,
    previous_focus: Option<WeakFocusHandle>,
}

impl CaptureGeneration for ActivePortDrag {
    fn generation(&self) -> u64 {
        self.generation
    }
}

impl OpenGpuiInteractionCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(InteractionState {
                capture: CaptureBookkeeping::new(false),
                focus_handle: None,
            })),
        }
    }

    pub(crate) fn set_focus_handle(&self, focus_handle: FocusHandle) {
        self.state.borrow_mut().focus_handle = Some(focus_handle);
    }

    pub(crate) fn active_owner(&self) -> Option<OpenGpuiPortDragOwner> {
        self.state
            .borrow()
            .capture
            .active()
            .map(|active| active.owner.clone())
    }

    pub(crate) fn requires_window_cleanup(&self) -> bool {
        self.state.borrow().capture.requires_cleanup()
    }

    pub(crate) fn rebind_port(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        hitbox: HitboxId,
        window: &mut Window,
    ) {
        if !mount.is_live() {
            return;
        }
        let rebound = {
            let mut state = self.state.borrow_mut();
            let Some(active) = state.capture.active_mut() else {
                return;
            };
            if &active.owner != owner || !active.mount.is_same_mount(mount) {
                return;
            }
            active.hitbox = hitbox;
            true
        };
        if rebound {
            window.capture_pointer(hitbox);
        }
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
        if self.state.borrow().capture.active().is_some() {
            return true;
        }

        let Some(generation) = self.allocate_generation() else {
            dispatch_cancelled_start(
                &owner,
                &listener,
                OpenGpuiPortDragCancelReason::InteractionUnavailable,
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
                invalidate_generation(&weak_state, generation);
            });
        let lease = match lease {
            Ok(lease) => lease,
            Err(error) => {
                dispatch_cancelled_start(
                    &owner,
                    &listener,
                    cancel_reason_for_acquire_error(error),
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
            phase: OpenGpuiPortDragPhase::Start,
            position: event.position,
            modifiers: event.modifiers,
        };
        self.state.borrow_mut().capture.activate(ActivePortDrag {
            generation,
            owner,
            mount,
            listener: listener.clone(),
            lease,
            previous_focus,
            hitbox,
            position: event.position,
            modifiers: event.modifiers,
        });
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
        let drained = self.drain_pending(window, cx);
        let mut state = self.state.borrow_mut();
        if state.capture.active().is_some() {
            return true;
        }
        if state.capture.suppression() && event.button == MouseButton::Left {
            state.capture.set_suppression(false);
            drop(state);
            window.release_pointer();
            return false;
        }
        drained || state.capture.suppression()
    }

    pub(crate) fn handle_mouse_move(
        &self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        let dispatch = {
            let mut state = self.state.borrow_mut();
            state.capture.active_mut().map(|active| {
                active.position = event.position;
                active.modifiers = event.modifiers;
                (
                    active.listener.clone(),
                    OpenGpuiPortDragEvent {
                        owner: active.owner.clone(),
                        phase: OpenGpuiPortDragPhase::Move,
                        position: event.position,
                        modifiers: event.modifiers,
                    },
                )
            })
        };
        if let Some((listener, event)) = dispatch {
            listener(OpenGpuiPortEvent::Pointer(event), window, cx);
            window.refresh();
            return true;
        }
        drained || self.state.borrow().capture.suppression()
    }

    pub(crate) fn handle_mouse_up(
        &self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        if event.button != MouseButton::Left {
            return drained || self.state.borrow().capture.active().is_some();
        }

        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            self.finish_active(
                active,
                OpenGpuiPortDragPhase::Release,
                event.position,
                event.modifiers,
                window,
                cx,
            );
            self.state.borrow_mut().capture.set_suppression(false);
            return true;
        }

        let suppress = self.state.borrow().capture.suppression();
        if suppress {
            self.state.borrow_mut().capture.set_suppression(false);
            window.release_pointer();
        }
        drained || suppress
    }

    pub(crate) fn handle_scroll(
        &self,
        _event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        drained
            || self.state.borrow().capture.active().is_some()
            || self.state.borrow().capture.suppression()
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
        let drained = self.drain_pending(window, cx);
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            let position = active.position;
            let modifiers = active.modifiers;
            self.finish_active(
                active,
                OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
                position,
                modifiers,
                window,
                cx,
            );
            self.state.borrow_mut().capture.set_suppression(true);
            return true;
        }
        drained || self.state.borrow().capture.suppression()
    }

    pub(crate) fn cancel_deferred(&self, reason: OpenGpuiPortDragCancelReason) -> bool {
        let active = self.state.borrow_mut().capture.take_active();
        let Some(active) = active else {
            return false;
        };
        self.queue_cancellation(active, reason);
        true
    }

    pub(crate) fn disable_port_deferred(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &Window,
        cx: &mut App,
    ) -> bool {
        let active =
            {
                let mut state = self.state.borrow_mut();
                if state.capture.active().is_some_and(|active| {
                    &active.owner == owner && active.mount.is_same_mount(mount)
                }) {
                    state.capture.take_active()
                } else {
                    None
                }
            };
        let Some(active) = active else {
            return false;
        };

        self.queue_cancellation(active, OpenGpuiPortDragCancelReason::PortDisabled);
        self.schedule_pending_drain(window, cx);
        true
    }

    pub(crate) fn cancel_owner_deferred(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        reason: OpenGpuiPortDragCancelReason,
    ) -> bool {
        let active =
            {
                let mut state = self.state.borrow_mut();
                if state.capture.active().is_some_and(|active| {
                    &active.owner == owner && active.mount.is_same_mount(mount)
                }) {
                    state.capture.take_active()
                } else {
                    None
                }
            };
        let Some(active) = active else {
            return false;
        };
        self.queue_cancellation(active, reason);
        true
    }

    pub(crate) fn cancel_now(
        &self,
        reason: OpenGpuiPortDragCancelReason,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            let position = active.position;
            let modifiers = active.modifiers;
            self.finish_active(
                active,
                OpenGpuiPortDragPhase::Cancel(reason),
                position,
                modifiers,
                window,
                cx,
            );
            self.state.borrow_mut().capture.set_suppression(true);
            return true;
        }
        drained
    }

    pub(crate) fn drain_pending(&self, window: &mut Window, cx: &mut App) -> bool {
        let pending = self.state.borrow_mut().capture.take_pending();
        if pending.is_empty() {
            return false;
        }

        window.release_pointer();
        for pending in pending {
            settle_focus(
                pending.event.phase,
                pending.previous_focus.as_ref(),
                self.state.borrow().focus_handle.as_ref(),
                window,
                cx,
            );
            (pending.listener)(OpenGpuiPortEvent::Pointer(pending.event), window, cx);
        }
        window.refresh();
        true
    }

    pub(crate) fn schedule_pending_drain(&self, window: &Window, cx: &mut App) -> bool {
        let should_schedule = self.state.borrow_mut().capture.prepare_pending_drain();
        if !should_schedule {
            return false;
        }

        let coordinator = self.clone();
        window.defer(cx, move |window, cx| {
            coordinator.drain_pending(window, cx);
        });
        true
    }

    fn allocate_generation(&self) -> Option<u64> {
        self.state.borrow_mut().capture.allocate_generation()
    }

    fn finish_active(
        &self,
        active: ActivePortDrag,
        phase: OpenGpuiPortDragPhase,
        position: Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) {
        let event = OpenGpuiPortDragEvent {
            owner: active.owner,
            phase,
            position,
            modifiers,
        };
        let listener = active.listener;
        let previous_focus = active.previous_focus;
        drop(active.lease);
        window.release_pointer();
        settle_focus(
            phase,
            previous_focus.as_ref(),
            self.state.borrow().focus_handle.as_ref(),
            window,
            cx,
        );
        listener(OpenGpuiPortEvent::Pointer(event), window, cx);
        window.refresh();
    }

    fn queue_cancellation(&self, active: ActivePortDrag, reason: OpenGpuiPortDragCancelReason) {
        let event = OpenGpuiPortDragEvent {
            owner: active.owner,
            phase: OpenGpuiPortDragPhase::Cancel(reason),
            position: active.position,
            modifiers: active.modifiers,
        };
        drop(active.lease);
        self.state.borrow_mut().capture.enqueue(
            PendingCancellation {
                event,
                listener: active.listener,
                previous_focus: active.previous_focus,
            },
            true,
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
}

/// Scene-level gesture owner. Native controls and real ports are handled by their own layers
/// before these owners are allowed to claim the pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OpenGpuiSceneGestureKind {
    Node(NodeInstanceKey),
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

#[derive(Clone)]
pub(crate) struct OpenGpuiSceneInteractionCoordinator {
    state: Rc<RefCell<SceneInteractionState>>,
}

type SceneCaptureBookkeeping =
    CaptureBookkeeping<ActiveSceneInteraction, PendingSceneCancellation, Option<MouseButton>>;

struct SceneInteractionState {
    capture: SceneCaptureBookkeeping,
    root_hitbox: Option<HitboxId>,
    focus_handle: Option<FocusHandle>,
}

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
    event: OpenGpuiSceneGestureEvent,
    listener: OpenGpuiSceneGestureListener,
    previous_focus: Option<WeakFocusHandle>,
}

impl CaptureGeneration for ActiveSceneInteraction {
    fn generation(&self) -> u64 {
        self.generation
    }
}

impl OpenGpuiSceneInteractionCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(SceneInteractionState {
                capture: CaptureBookkeeping::new(None),
                root_hitbox: None,
                focus_handle: None,
            })),
        }
    }

    pub(crate) fn requires_window_cleanup(&self) -> bool {
        self.state.borrow().capture.requires_cleanup()
    }

    pub(crate) fn set_root_hitbox(&self, hitbox: HitboxId, window: &mut Window) {
        let rebound = {
            let mut state = self.state.borrow_mut();
            state.root_hitbox = Some(hitbox);
            if let Some(active) = state.capture.active_mut() {
                active.hitbox = hitbox;
                true
            } else {
                false
            }
        };
        if rebound {
            window.capture_pointer(hitbox);
        }
    }

    pub(crate) fn set_focus_handle(&self, focus_handle: FocusHandle) {
        self.state.borrow_mut().focus_handle = Some(focus_handle);
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
        if self.state.borrow().capture.active().is_some() {
            return true;
        }
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
                    invalidate_scene_generation(&weak_state, generation)
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
            .activate(ActiveSceneInteraction {
                generation,
                kind,
                button,
                listener: listener.clone(),
                lease,
                hitbox,
                previous_focus,
                position: event.position,
                modifiers: event.modifiers,
            });
        listener(start, window, cx);
        window.refresh();
        true
    }

    pub(crate) fn handle_mouse_down(
        &self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        let mut state = self.state.borrow_mut();
        if state.capture.active().is_some() {
            return true;
        }
        if state.capture.suppression() == Some(event.button) {
            state.capture.set_suppression(None);
            drop(state);
            window.release_pointer();
            return false;
        }
        drained || state.capture.suppression().is_some()
    }

    pub(crate) fn handle_mouse_move(
        &self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        let dispatch = {
            let mut state = self.state.borrow_mut();
            state.capture.active_mut().map(|active| {
                active.position = event.position;
                active.modifiers = event.modifiers;
                (
                    active.listener.clone(),
                    OpenGpuiSceneGestureEvent {
                        kind: active.kind.clone(),
                        phase: OpenGpuiSceneGesturePhase::Move,
                        position: event.position,
                        modifiers: event.modifiers,
                    },
                )
            })
        };
        if let Some((listener, event)) = dispatch {
            listener(event, window, cx);
            window.refresh();
            return true;
        }
        drained || self.state.borrow().capture.suppression().is_some()
    }

    pub(crate) fn handle_mouse_up(
        &self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let drained = self.drain_pending(window, cx);
        let active_button = self
            .state
            .borrow()
            .capture
            .active()
            .map(|active| active.button);
        if let Some(button) = active_button {
            if button != event.button {
                return true;
            }
            let active = self.state.borrow_mut().capture.take_active();
            if let Some(active) = active {
                self.finish_active(
                    active,
                    OpenGpuiSceneGesturePhase::Release,
                    event.position,
                    event.modifiers,
                    window,
                    cx,
                );
            }
            self.state.borrow_mut().capture.set_suppression(None);
            return true;
        }
        let suppress = self.state.borrow().capture.suppression() == Some(event.button);
        if suppress {
            self.state.borrow_mut().capture.set_suppression(None);
            window.release_pointer();
        }
        drained || suppress
    }

    pub(crate) fn handle_scroll(&self, window: &mut Window, cx: &mut App) -> bool {
        let drained = self.drain_pending(window, cx);
        drained
            || self.state.borrow().capture.active().is_some()
            || self.state.borrow().capture.suppression().is_some()
    }

    pub(crate) fn handle_pinch(&self, window: &mut Window, cx: &mut App) -> bool {
        self.handle_scroll(window, cx)
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
        let drained = self.drain_pending(window, cx);
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            let position = active.position;
            let modifiers = active.modifiers;
            let button = active.button;
            self.finish_active(
                active,
                OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::Escape),
                position,
                modifiers,
                window,
                cx,
            );
            self.state
                .borrow_mut()
                .capture
                .set_suppression(Some(button));
            return true;
        }
        drained || self.state.borrow().capture.suppression().is_some()
    }

    pub(crate) fn cancel_deferred(&self, reason: OpenGpuiSceneGestureCancelReason) -> bool {
        let active = self.state.borrow_mut().capture.take_active();
        let Some(active) = active else {
            return false;
        };
        self.queue_cancellation(active, reason);
        true
    }

    pub(crate) fn cancel_owner_deferred(
        &self,
        owner: &NodeInstanceKey,
        reason: OpenGpuiSceneGestureCancelReason,
    ) -> bool {
        let active = {
            let mut state = self.state.borrow_mut();
            if state.capture.active().is_some_and(|active| {
                matches!(&active.kind, OpenGpuiSceneGestureKind::Node(current) if current == owner)
            }) {
                state.capture.take_active()
            } else {
                None
            }
        };
        let Some(active) = active else {
            return false;
        };
        self.queue_cancellation(active, reason);
        true
    }

    pub(crate) fn cancel_for_host_close(&self, window: &mut Window, cx: &mut App) -> bool {
        let active = self.state.borrow_mut().capture.take_active();
        if let Some(active) = active {
            self.queue_cancellation(active, OpenGpuiSceneGestureCancelReason::HostClosed);
        }
        let pending = self.state.borrow_mut().capture.take_pending();
        if pending.is_empty() {
            return false;
        }

        window.release_pointer();
        window.blur();
        window.refresh();
        window.defer(cx, move |window, cx| {
            for pending in pending {
                (pending.listener)(pending.event, window, cx);
            }
            window.refresh();
        });
        true
    }

    pub(crate) fn drain_pending(&self, window: &mut Window, cx: &mut App) -> bool {
        let pending = self.state.borrow_mut().capture.take_pending();
        if pending.is_empty() {
            return false;
        }
        window.release_pointer();
        for pending in pending {
            let interaction_focus = self.state.borrow().focus_handle.clone();
            settle_scene_focus(
                pending.event.phase(),
                pending.previous_focus.as_ref(),
                interaction_focus.as_ref(),
                window,
                cx,
            );
            (pending.listener)(pending.event, window, cx);
        }
        window.refresh();
        true
    }

    pub(crate) fn schedule_pending_drain(&self, window: &Window, cx: &mut App) -> bool {
        let should_schedule = self.state.borrow_mut().capture.prepare_pending_drain();
        if !should_schedule {
            return false;
        }
        let coordinator = self.clone();
        window.defer(cx, move |window, cx| {
            coordinator.drain_pending(window, cx);
        });
        true
    }

    fn allocate_generation(&self) -> Option<u64> {
        self.state.borrow_mut().capture.allocate_generation()
    }

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

    fn finish_active(
        &self,
        active: ActiveSceneInteraction,
        phase: OpenGpuiSceneGesturePhase,
        position: Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut App,
    ) {
        let event = OpenGpuiSceneGestureEvent {
            kind: active.kind,
            phase,
            position,
            modifiers,
        };
        let previous_focus = active.previous_focus;
        let interaction_focus = self.state.borrow().focus_handle.clone();
        drop(active.lease);
        window.release_pointer();
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

    fn queue_cancellation(
        &self,
        active: ActiveSceneInteraction,
        reason: OpenGpuiSceneGestureCancelReason,
    ) {
        let event = OpenGpuiSceneGestureEvent {
            kind: active.kind,
            phase: OpenGpuiSceneGesturePhase::Cancel(reason),
            position: active.position,
            modifiers: active.modifiers,
        };
        let button = active.button;
        let previous_focus = active.previous_focus;
        drop(active.lease);
        self.state.borrow_mut().capture.enqueue(
            PendingSceneCancellation {
                event,
                listener: active.listener,
                previous_focus,
            },
            Some(button),
        );
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

fn invalidate_scene_generation(state: &Weak<RefCell<SceneInteractionState>>, generation: u64) {
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
    let event = OpenGpuiSceneGestureEvent {
        kind: active.kind,
        phase: OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::OwnerRetired),
        position: active.position,
        modifiers: active.modifiers,
    };
    let button = active.button;
    let previous_focus = active.previous_focus;
    drop(active.lease);
    state.borrow_mut().capture.enqueue(
        PendingSceneCancellation {
            event,
            listener: active.listener,
            previous_focus,
        },
        Some(button),
    );
}

pub(crate) fn interaction_root(
    id: impl Into<ElementId>,
    coordinator: OpenGpuiInteractionCoordinator,
    scene_coordinator: OpenGpuiSceneInteractionCoordinator,
    focus_handle: FocusHandle,
    child: impl IntoElement,
) -> impl IntoElement {
    OpenGpuiInteractionRoot {
        id: id.into(),
        coordinator,
        scene_coordinator,
        focus_handle,
        child: Some(child.into_any_element()),
    }
}

struct OpenGpuiInteractionRoot {
    id: ElementId,
    coordinator: OpenGpuiInteractionCoordinator,
    scene_coordinator: OpenGpuiSceneInteractionCoordinator,
    focus_handle: FocusHandle,
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
        self.coordinator.schedule_pending_drain(window, cx);
        self.scene_coordinator.schedule_pending_drain(window, cx);
        self.scene_coordinator.set_root_hitbox(hitbox.id, window);
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
        let coordinator = self.coordinator.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && coordinator.handle_mouse_down(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && scene_coordinator.handle_mouse_down(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
        let coordinator = self.coordinator.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && coordinator.handle_mouse_move(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && scene_coordinator.handle_mouse_move(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
        let coordinator = self.coordinator.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && coordinator.handle_mouse_up(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && scene_coordinator.handle_mouse_up(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
        let coordinator = self.coordinator.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && coordinator.handle_scroll(event, window, cx) {
                cx.stop_propagation();
            }
        });
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_mouse_event(move |_: &ScrollWheelEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && scene_coordinator.handle_scroll(window, cx) {
                cx.stop_propagation();
            }
        });
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_mouse_event(move |_: &PinchEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && scene_coordinator.handle_pinch(window, cx) {
                cx.stop_propagation();
            }
        });
        let coordinator = self.coordinator.clone();
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture && coordinator.handle_key_down(event, window, cx) {
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
        let scene_coordinator = self.scene_coordinator.clone();
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Capture
                && scene_coordinator.handle_key_down(event, window, cx)
            {
                cx.stop_propagation();
            }
        });
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

#[derive(Clone, Copy)]
enum GuardMode {
    Embedded,
    Overlay,
}

struct OpenGpuiInteractionGuardElement {
    child: Option<AnyElement>,
    mode: GuardMode,
}

impl OpenGpuiInteractionGuardElement {
    fn new(child: impl IntoElement, mode: GuardMode) -> Self {
        Self {
            child: Some(child.into_any_element()),
            mode,
        }
    }
}

impl Element for OpenGpuiInteractionGuardElement {
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
        let mut child = self.child.take().expect("interaction guard child missing");
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
        let behavior = match self.mode {
            GuardMode::Embedded => HitboxBehavior::Normal,
            GuardMode::Overlay => HitboxBehavior::BlockMouse,
        };
        let hitbox = window.insert_hitbox(bounds, behavior);
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

impl IntoElement for OpenGpuiInteractionGuardElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn invalidate_generation(state: &Weak<RefCell<InteractionState>>, generation: u64) {
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
    let event = OpenGpuiPortDragEvent {
        owner: active.owner,
        phase: OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
        position: active.position,
        modifiers: active.modifiers,
    };
    drop(active.lease);
    state.borrow_mut().capture.enqueue(
        PendingCancellation {
            event,
            listener: active.listener,
            previous_focus: active.previous_focus,
        },
        true,
    );
}

fn settle_focus(
    phase: OpenGpuiPortDragPhase,
    previous_focus: Option<&WeakFocusHandle>,
    interaction_focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    match phase {
        OpenGpuiPortDragPhase::Release
        | OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape) => {
            restore_previous_focus(previous_focus, interaction_focus, window, cx);
        }
        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::PortDisabled) => {
            if interaction_focus.is_some_and(|focus| focus.is_focused(window)) {
                window.blur();
            }
        }
        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired) => {
            if let Some(interaction_focus) =
                interaction_focus.filter(|focus| focus.is_focused(window))
            {
                window.focus(interaction_focus, cx);
            }
        }
        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::HostClosed) => window.blur(),
        OpenGpuiPortDragPhase::Start
        | OpenGpuiPortDragPhase::Move
        | OpenGpuiPortDragPhase::Cancel(
            OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded
            | OpenGpuiPortDragCancelReason::InteractionUnavailable,
        ) => {}
    }
}

fn settle_scene_focus(
    phase: OpenGpuiSceneGesturePhase,
    previous_focus: Option<&WeakFocusHandle>,
    interaction_focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    if matches!(
        phase,
        OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::HostClosed)
    ) {
        window.blur();
        return;
    }
    if !interaction_focus.is_some_and(|focus| focus.is_focused(window)) {
        return;
    }
    if let Some(previous_focus) = previous_focus.and_then(WeakFocusHandle::upgrade) {
        window.focus(&previous_focus, cx);
    }
}

fn restore_previous_focus(
    previous_focus: Option<&WeakFocusHandle>,
    interaction_focus: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut App,
) {
    if interaction_focus.is_some_and(|focus| !focus.is_focused(window)) {
        return;
    }
    if let Some(previous_focus) = previous_focus.and_then(WeakFocusHandle::upgrade) {
        window.focus(&previous_focus, cx);
    } else {
        window.blur();
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
    position: Point<Pixels>,
    modifiers: Modifiers,
    window: &mut Window,
    cx: &mut App,
) {
    listener(
        OpenGpuiPortEvent::Pointer(OpenGpuiPortDragEvent {
            owner: owner.clone(),
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
    use std::fmt::Debug;

    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    struct TestActive {
        generation: u64,
    }

    impl CaptureGeneration for TestActive {
        fn generation(&self) -> u64 {
            self.generation
        }
    }

    #[test]
    fn port_and_scene_coordinators_use_the_same_bookkeeping_invariants() {
        fn assert_initial_invariants<Active, Pending, Suppression>(
            capture: &mut CaptureBookkeeping<Active, Pending, Suppression>,
        ) {
            assert_eq!(capture.allocate_generation(), Some(1));
            assert!(!capture.requires_cleanup());
            assert!(!capture.prepare_pending_drain());
        }

        let port = OpenGpuiInteractionCoordinator::new();
        let scene = OpenGpuiSceneInteractionCoordinator::new();

        {
            let mut state = port.state.borrow_mut();
            assert_initial_invariants(&mut state.capture);
        }
        {
            let mut state = scene.state.borrow_mut();
            assert_initial_invariants(&mut state.capture);
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
}
