use std::rc::Rc;

use jellyflow::core::CanvasPoint;
use jellyflow::runtime::CanvasInteractionPhase;
use jellyflow::runtime::runtime::viewport::{
    ViewportPanRequest, ViewportZoomRequest, pan_viewport, zoom_viewport,
};
use jellyflow::runtime::{
    CanvasInteractionError, CanvasInteractionUpdate, CanvasPressFallback, CanvasPressTarget,
    InteractionOwner, NodeDragStart, NodeInstanceKey, SelectionBaseline, SelectionMode, ViewDelta,
};
use open_gpui::{
    Context, Modifiers, MouseMoveEvent, PinchEvent, Pixels, ScrollWheelEvent, Window, px,
};

use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGestureKind,
    OpenGpuiSceneGestureListener, OpenGpuiSceneGesturePhase,
};

use super::OpenGpuiSceneHost;

const SCENE_WHEEL_LINE_HEIGHT: f32 = 16.0;

impl OpenGpuiSceneHost {
    pub(super) fn handle_scene_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.canvas_interaction.presentation().phase() != CanvasInteractionPhase::Idle {
            return;
        }
        let Some(position) = self.interaction_position(event.position) else {
            return;
        };
        let mut hovered_node = None;
        for (instance, mounted) in self.mounts.active_in_order() {
            let bounds = mounted.bounds;
            let point = position.canvas();
            if point.x >= bounds.origin.x
                && point.y >= bounds.origin.y
                && point.x <= bounds.origin.x + bounds.size.width
                && point.y <= bounds.origin.y + bounds.size.height
            {
                hovered_node = Some(instance.node());
            }
        }
        if self.hovered_node != hovered_node {
            self.hovered_node = hovered_node;
            cx.notify();
        }
        let result = if hovered_node.is_some() {
            self.canvas_interaction.clear_edge_hover()
        } else {
            self.canvas_interaction.request_edge_pick(position)
        };
        self.apply_scene_interaction_result(result, window, cx);
    }

    pub(super) fn handle_scene_pointer_exit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.hovered_node.take().is_some() {
            cx.notify();
        }
        if let Ok(update) = self.canvas_interaction.clear_edge_hover() {
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    #[cfg(test)]
    pub(super) fn active_scene_gesture(&self) -> bool {
        matches!(
            self.canvas_interaction.presentation().phase(),
            CanvasInteractionPhase::Armed
                | CanvasInteractionPhase::NodeDrag
                | CanvasInteractionPhase::Pan
                | CanvasInteractionPhase::Marquee
        )
    }

    pub(super) fn host_scene_listener(cx: &Context<Self>) -> OpenGpuiSceneGestureListener {
        let host = cx.weak_entity();
        Rc::new(move |event, window, cx| {
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.handle_scene_gesture(event, window, cx);
            });
        })
    }

    fn handle_scene_gesture(
        &mut self,
        event: OpenGpuiSceneGestureEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        self.scene_evidence.record_scene_event(&event);

        match event.phase() {
            OpenGpuiSceneGesturePhase::Start => {
                if !self.start_scene_gesture(&event, window, cx) {
                    self.scene_interactions
                        .cancel_deferred(OpenGpuiSceneGestureCancelReason::InteractionUnavailable);
                    cx.notify();
                }
            }
            OpenGpuiSceneGesturePhase::Move => {
                self.move_scene_gesture(&event, window, cx);
            }
            OpenGpuiSceneGesturePhase::Release => {
                self.release_scene_gesture(&event, window, cx);
            }
            OpenGpuiSceneGesturePhase::Cancel(reason) => {
                self.cancel_scene_gesture_for_reason(&event, reason, window, cx);
            }
        }
    }

    fn start_scene_gesture(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(position) = self.interaction_position(event.position()) else {
            return false;
        };
        let Some(target) = self.scene_press_target(event) else {
            return false;
        };
        let result = self.canvas_interaction.press(target, position);
        if result.is_err() && matches!(event.kind(), OpenGpuiSceneGestureKind::EdgeReconnect(..)) {
            self.connection_owner = None;
        }
        self.apply_scene_interaction_result(result, window, cx)
    }

    fn move_scene_gesture(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.scene_event_owner_is_current(event.kind()) {
            self.cancel_scene_gesture(window, cx);
            self.scene_interactions
                .cancel_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
            return;
        }
        let Some(position) = self.interaction_position(event.position()) else {
            return;
        };
        let result = self.canvas_interaction.move_to(position);
        if !self.apply_scene_interaction_result(result, window, cx) {
            self.cancel_scene_gesture(window, cx);
            self.scene_interactions
                .cancel_deferred(OpenGpuiSceneGestureCancelReason::InteractionUnavailable);
        }
    }

    fn release_scene_gesture(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.scene_event_owner_is_current(event.kind()) {
            self.cancel_scene_gesture(window, cx);
            return;
        }
        let Some(position) = self.interaction_position(event.position()) else {
            self.cancel_scene_gesture(window, cx);
            return;
        };

        // The final native pointer position may arrive only with mouse-up. Feed it through the
        // move transition before release so node delta, marquee bounds, and pan stay continuous.
        let moved = self.canvas_interaction.move_to(position);
        if !self.apply_scene_interaction_result(moved, window, cx) {
            self.cancel_scene_gesture(window, cx);
            return;
        }
        let released = self.canvas_interaction.release(position);
        if !self.apply_scene_interaction_result(released, window, cx) {
            self.cancel_scene_gesture(window, cx);
        }
    }

    fn cancel_scene_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let update = self.canvas_interaction.cancel();
        self.discard_pending_interaction_submission();
        self.apply_canvas_interaction_update(update, window, cx);
    }

    fn cancel_scene_gesture_for_reason(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
        reason: OpenGpuiSceneGestureCancelReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (update, settle_pending) = match (reason, event.kind()) {
            (
                OpenGpuiSceneGestureCancelReason::OwnerRetired,
                OpenGpuiSceneGestureKind::Node(instance),
            ) if self.scene_node_owner_is_current(instance) => (
                self.canvas_interaction
                    .owner_retired(InteractionOwner::Node(instance.node())),
                true,
            ),
            (OpenGpuiSceneGestureCancelReason::OwnerRetired, _) => {
                (CanvasInteractionUpdate::default(), false)
            }
            (OpenGpuiSceneGestureCancelReason::HostClosed, _) => {
                (self.canvas_interaction.close(), true)
            }
            (
                OpenGpuiSceneGestureCancelReason::InteractionUnavailable
                | OpenGpuiSceneGestureCancelReason::ProtectedCapacityExceeded,
                _,
            ) => (CanvasInteractionUpdate::default(), false),
            _ => (self.canvas_interaction.cancel(), true),
        };
        if settle_pending {
            self.discard_pending_interaction_submission();
        }
        self.apply_canvas_interaction_update(update, window, cx);
    }

    fn apply_scene_interaction_result(
        &mut self,
        result: Result<CanvasInteractionUpdate, CanvasInteractionError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            Ok(update) => {
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                cx.notify();
                false
            }
        }
    }

    fn scene_press_target(
        &mut self,
        event: &OpenGpuiSceneGestureEvent,
    ) -> Option<CanvasPressTarget> {
        let selection_mode = scene_selection_mode(event);
        match event.kind() {
            OpenGpuiSceneGestureKind::Node(instance) => self
                .node_drag_start(instance, selection_mode)
                .map(CanvasPressTarget::NodeDrag),
            OpenGpuiSceneGestureKind::EdgeReconnect(edge, endpoint) => {
                self.edge_reconnect_press_target(*edge, *endpoint)
            }
            OpenGpuiSceneGestureKind::Pan => Some(CanvasPressTarget::Pan),
            OpenGpuiSceneGestureKind::Selection => {
                let selection = self.view.selection().ok()?;
                Some(CanvasPressTarget::Canvas(CanvasPressFallback::Marquee {
                    selection_mode,
                    baseline: SelectionBaseline::new(
                        selection.nodes().iter().copied(),
                        selection.edges().iter().copied(),
                        selection.groups().iter().copied(),
                    ),
                }))
            }
        }
    }

    fn node_drag_start(
        &self,
        instance: &NodeInstanceKey,
        selection_mode: SelectionMode,
    ) -> Option<NodeDragStart> {
        if !self.scene_node_owner_is_current(instance) {
            return None;
        }

        let clicked = instance.node();
        self.view
            .node_drag_start(clicked, selection_mode)
            .ok()
            .flatten()
    }

    fn scene_event_owner_is_current(&self, kind: &OpenGpuiSceneGestureKind) -> bool {
        match kind {
            OpenGpuiSceneGestureKind::Node(instance) => self.scene_node_owner_is_current(instance),
            OpenGpuiSceneGestureKind::EdgeReconnect(edge, endpoint) => {
                self.edge_reconnect_is_current(*edge, *endpoint)
            }
            OpenGpuiSceneGestureKind::Pan | OpenGpuiSceneGestureKind::Selection => true,
        }
    }

    fn scene_node_owner_is_current(&self, instance: &NodeInstanceKey) -> bool {
        instance.view() == self.view.id()
            && self.mounts.retained_instance(instance.node()) == Some(instance)
            && self
                .mounts
                .get(instance)
                .is_some_and(|mounted| mounted.mount.is_live())
            && matches!(
                self.view.node_incarnation(instance.node()),
                Ok(Some(current)) if current == instance.incarnation()
            )
    }

    pub(super) fn handle_scene_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(transform) = self.view.viewport_transform() else {
            return false;
        };
        let Some(anchor) = self.scene_screen_position(event.position) else {
            return false;
        };
        let delta = event.delta.pixel_delta(px(SCENE_WHEEL_LINE_HEIGHT));
        let delta = ViewDelta {
            x: delta.x.as_f32(),
            y: delta.y.as_f32(),
        };
        let next = if self.navigation.profile().wheel_pans()
            || event.modifiers.shift
            || delta.x.abs() > delta.y.abs()
        {
            pan_viewport(transform, ViewportPanRequest::new(delta))
        } else {
            zoom_viewport(
                transform,
                ViewportZoomRequest::new(
                    anchor,
                    transform.zoom * (delta.y * self.navigation.wheel_zoom_sensitivity()).exp(),
                    self.navigation.min_zoom(),
                    self.navigation.max_zoom(),
                ),
            )
        };
        let Some(next) = next else {
            return false;
        };
        if self.view.set_viewport(next.pan, next.zoom).is_err() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn handle_scene_pinch(
        &mut self,
        event: &PinchEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(transform) = self.view.viewport_transform() else {
            return false;
        };
        let Some(anchor) = self.scene_screen_position(event.position) else {
            return false;
        };
        let Some(next) = zoom_viewport(
            transform,
            ViewportZoomRequest::new(
                anchor,
                transform.zoom * (1.0 + event.delta).max(0.1),
                self.navigation.min_zoom(),
                self.navigation.max_zoom(),
            ),
        ) else {
            return false;
        };
        if self.view.set_viewport(next.pan, next.zoom).is_err() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn scene_screen_position(
        &self,
        position: open_gpui::Point<Pixels>,
    ) -> Option<CanvasPoint> {
        let origin = self.scene_origin?;
        let screen = CanvasPoint {
            x: (position.x - origin.x).as_f32(),
            y: (position.y - origin.y).as_f32(),
        };
        screen.is_finite().then_some(screen)
    }
}

fn scene_selection_mode(event: &OpenGpuiSceneGestureEvent) -> SelectionMode {
    selection_mode_for_modifiers(event.modifiers())
}

pub(super) fn selection_mode_for_modifiers(modifiers: Modifiers) -> SelectionMode {
    if modifiers.shift || modifiers.secondary() {
        SelectionMode::Add
    } else {
        SelectionMode::Replace
    }
}
