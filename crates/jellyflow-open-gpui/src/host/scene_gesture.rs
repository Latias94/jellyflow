use std::rc::Rc;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_runtime::interaction::CanvasInteractionPhase;
use jellyflow_runtime::viewport::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest, pan_viewport, zoom_viewport,
};
use jellyflow_runtime::{
    geometry::ViewDelta, geometry::ViewPoint, interaction::CanvasInteractionError,
    interaction::CanvasInteractionUpdate, interaction::CanvasPressFallback,
    interaction::CanvasPressTarget, interaction::InteractionOwner, interaction::NodeDragStart,
    interaction::SelectionMode, view::GeometrySnapshot, view::NodeInstanceKey,
    viewport::ViewportPointerSample,
};
use open_gpui::{
    Context, MouseDownEvent, MouseMoveEvent, PinchEvent, Pixels, ScrollWheelEvent, Window, px,
};

use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGestureKind,
    OpenGpuiSceneGestureListener, OpenGpuiSceneGesturePhase,
};

use super::{
    OpenGpuiBlankCanvasContextRequest, OpenGpuiSceneHost, PendingBlankCanvasContext,
    node_lifecycle::MountedNode,
};

const SCENE_WHEEL_LINE_HEIGHT: f32 = 16.0;

fn mounted_node_interaction_bounds(
    instance: &NodeInstanceKey,
    mounted: &MountedNode,
    geometry: Option<&GeometrySnapshot>,
    transform: ViewportTransform,
) -> CanvasRect {
    if let Some(bounds) = geometry
        .and_then(|geometry| geometry.node(instance.node()))
        .filter(|geometry| {
            geometry.is_fresh()
                && geometry.lifetime() == instance.lifetime()
                && geometry.renderer() == instance.renderer()
        })
        .map(|geometry| geometry.bounds())
    {
        return bounds;
    }

    let screen_size = mounted.semantic_zoom.screen_size(
        transform.zoom,
        mounted.graph_presentation(),
        mounted.registration_policy.scale_policy(),
    );
    CanvasRect {
        origin: mounted.bounds.origin,
        size: CanvasSize {
            width: screen_size.width / transform.zoom,
            height: screen_size.height / transform.zoom,
        },
    }
}

impl OpenGpuiSceneHost {
    pub(super) fn request_blank_canvas_context(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_blank_canvas_context.is_some() {
            return true;
        }
        let Some(handler) = self.blank_canvas_context_handler.clone() else {
            return false;
        };
        if self.interaction_owner.presentation().phase() != CanvasInteractionPhase::Idle
            || self.interaction_owner.native_capability().overlay_active()
        {
            return false;
        }

        let Some(sample) = self.viewport_pointer_sample(event.position) else {
            return false;
        };
        let Some(position) = sample_canvas_point(sample) else {
            return false;
        };
        if self.scene_point_hits_node_or_port(position) != Some(false) {
            return false;
        }

        self.pending_blank_canvas_context = Some(PendingBlankCanvasContext {
            handler,
            request: OpenGpuiBlankCanvasContextRequest {
                position,
                button: event.button,
                modifiers: event.modifiers,
            },
        });
        let result = self.interaction_owner.request_edge_pick(sample);
        if !self.apply_scene_interaction_result(result, window, cx) {
            self.pending_blank_canvas_context = None;
            return false;
        }
        self.finish_blank_canvas_context_query(window, cx);
        true
    }

    fn scene_point_hits_node_or_port(&self, point: CanvasPoint) -> Option<bool> {
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        let graph = self.view.graph_snapshot().ok()?;
        let geometry = self.view.geometry_snapshot().ok()?;

        for (instance, mounted) in self.mounts.active_in_order() {
            let bounds =
                mounted_node_interaction_bounds(instance, mounted, Some(&geometry), transform);
            if canvas_rect_contains(bounds, point) {
                return Some(true);
            }

            let node = graph.graph().node(&instance.node())?;
            if node.ports.iter().any(|port| {
                geometry
                    .port(*port)
                    .and_then(|geometry| geometry.hit_bounds())
                    .is_some_and(|bounds| canvas_rect_contains(bounds, point))
            }) {
                return Some(true);
            }
        }

        Some(false)
    }

    pub(super) fn finish_blank_canvas_context_query(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_blank_canvas_context.is_none()
            || self.interaction_owner.has_pending_queries()
            || self.interaction_owner.query_resolution_active()
        {
            return;
        }
        let presentation = self.interaction_owner.presentation();
        if presentation.phase() == CanvasInteractionPhase::Armed {
            return;
        }
        let pending = self.pending_blank_canvas_context.take();
        if presentation.phase() != CanvasInteractionPhase::Idle
            || presentation.terminal().is_some()
            || presentation.hovered_edge().is_some()
        {
            return;
        }
        let Some(PendingBlankCanvasContext { handler, request }) = pending else {
            return;
        };
        window.defer(cx, move |window, cx| handler(request, window, cx));
    }

    pub(super) fn handle_scene_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_blank_canvas_context = None;
        if self.interaction_owner.presentation().phase() != CanvasInteractionPhase::Idle {
            return;
        }
        let Some(sample) = self.viewport_pointer_sample(event.position) else {
            return;
        };
        let Some(transform) = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())
        else {
            return;
        };
        let mut hovered_node = None;
        let mut hovered_port = None;
        let graph = self.view.graph_snapshot().ok();
        let geometry = self.view.geometry_snapshot().ok();
        for (instance, mounted) in self.mounts.active_in_order() {
            let bounds =
                mounted_node_interaction_bounds(instance, mounted, geometry.as_ref(), transform);
            let Some(point) = sample_canvas_point(sample) else {
                return;
            };
            if canvas_rect_contains(bounds, point) {
                hovered_node = Some(instance.node());
            }
            let Some(node) = graph
                .as_ref()
                .and_then(|snapshot| snapshot.graph().node(&instance.node()))
            else {
                continue;
            };
            for port in &node.ports {
                if geometry
                    .as_ref()
                    .and_then(|geometry| geometry.port(*port))
                    .and_then(|geometry| geometry.hit_bounds())
                    .is_some_and(|bounds| canvas_rect_contains(bounds, point))
                {
                    hovered_node = Some(instance.node());
                    hovered_port = Some(*port);
                }
            }
        }
        if self.hovered_node != hovered_node || self.hovered_port != hovered_port {
            self.hovered_node = hovered_node;
            self.hovered_port = hovered_port;
            cx.notify();
        }
        let result = if hovered_node.is_some() {
            self.interaction_owner.clear_edge_hover()
        } else {
            self.interaction_owner.request_edge_pick(sample)
        };
        self.apply_scene_interaction_result(result, window, cx);
    }

    pub(super) fn handle_scene_pointer_exit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_blank_canvas_context = None;
        let had_hovered_node = self.hovered_node.take().is_some();
        let had_hovered_port = self.hovered_port.take().is_some();
        if had_hovered_node || had_hovered_port {
            cx.notify();
        }
        if let Ok(update) = self.interaction_owner.clear_edge_hover() {
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    #[cfg(test)]
    pub(super) fn active_scene_gesture(&self) -> bool {
        matches!(
            self.interaction_owner.presentation().phase(),
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
        self.pending_blank_canvas_context = None;
        if self.interaction_owner.terminal() {
            return;
        }
        #[cfg(test)]
        self.scene_evidence.record_scene_event(&event);

        match event.phase() {
            OpenGpuiSceneGesturePhase::Start => {
                if !self.start_scene_gesture(&event, window, cx) {
                    self.interaction_owner.cancel_scene_deferred(
                        OpenGpuiSceneGestureCancelReason::InteractionUnavailable,
                    );
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
        if self
            .interaction_owner
            .keyboard_connection_source()
            .is_some()
        {
            let cancelled = self.interaction_owner.cancel();
            self.discard_pending_interaction_submission();
            if !self.apply_scene_interaction_result(cancelled, window, cx) {
                return false;
            }
        }
        let Some(sample) = self.viewport_pointer_sample(event.position()) else {
            return false;
        };
        if let OpenGpuiSceneGestureKind::EdgeReconnect(edge, endpoint) = event.kind() {
            if self.interaction_owner.connection_identity().is_some() {
                return true;
            }
            let Some(plan) = self.edge_reconnect_press_target(*edge, *endpoint) else {
                return false;
            };
            let (start, admission) = plan.into_parts();
            return match self
                .interaction_owner
                .press_connection(start, sample, admission)
            {
                Ok(Some(update)) => {
                    self.apply_canvas_interaction_update(update, window, cx);
                    true
                }
                Ok(None) => true,
                Err(error) => self.apply_scene_interaction_result(Err(error), window, cx),
            };
        }
        let Some(target) = self.scene_press_target(event) else {
            return false;
        };
        let result = self.interaction_owner.press(target, sample);
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
            self.interaction_owner
                .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
            return;
        }
        let Some(sample) = self.viewport_pointer_sample(event.position()) else {
            return;
        };
        let result = self.interaction_owner.move_to(sample);
        if !self.apply_scene_interaction_result(result, window, cx) {
            self.cancel_scene_gesture(window, cx);
            self.interaction_owner
                .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::InteractionUnavailable);
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
        let Some(sample) = self.viewport_pointer_sample(event.position()) else {
            self.cancel_scene_gesture(window, cx);
            return;
        };

        let released = self.interaction_owner.release(sample);
        if !self.apply_scene_interaction_result(released, window, cx) {
            self.cancel_scene_gesture(window, cx);
        }
    }

    fn cancel_scene_gesture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let update = self.interaction_owner.cancel();
        self.discard_pending_interaction_submission();
        self.apply_scene_interaction_result(update, window, cx);
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
                Ok(self
                    .interaction_owner
                    .owner_retired(InteractionOwner::Node(instance.lifetime()))),
                true,
            ),
            (OpenGpuiSceneGestureCancelReason::OwnerRetired, _) => {
                (Ok(CanvasInteractionUpdate::default()), false)
            }
            (OpenGpuiSceneGestureCancelReason::HostClosed, _) => {
                (Ok(self.interaction_owner.close_semantic()), true)
            }
            (
                OpenGpuiSceneGestureCancelReason::InteractionUnavailable
                | OpenGpuiSceneGestureCancelReason::ProtectedCapacityExceeded,
                _,
            ) => (Ok(CanvasInteractionUpdate::default()), false),
            _ => (self.interaction_owner.cancel(), true),
        };
        if settle_pending {
            self.discard_pending_interaction_submission();
        }
        self.apply_scene_interaction_result(update, window, cx);
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
        let selection_mode = self
            .navigation
            .input_bindings()
            .selection()
            .selection_mode(event.modifiers());
        match event.kind() {
            OpenGpuiSceneGestureKind::Node(instance) => self
                .node_drag_start(instance, selection_mode)
                .map(CanvasPressTarget::NodeDrag),
            OpenGpuiSceneGestureKind::EdgeReconnect(..) => None,
            OpenGpuiSceneGestureKind::Pan => Some(CanvasPressTarget::Pan),
            OpenGpuiSceneGestureKind::Selection => {
                let baseline = self.view.selection_baseline().ok()?;
                Some(CanvasPressTarget::Canvas(CanvasPressFallback::Marquee {
                    selection_mode,
                    baseline,
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

        self.view
            .node_drag_start(instance.lifetime(), selection_mode)
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
                self.view.node_lifetime_key(instance.node()),
                Ok(Some(current)) if current == instance.lifetime()
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
        let next = if self.navigation.input_bindings().wheel_pans()
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
        if self.view.set_viewport(next).is_err() {
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
        if self.view.set_viewport(next).is_err() {
            return false;
        }
        cx.notify();
        true
    }

    pub(super) fn scene_screen_position(
        &self,
        position: open_gpui::Point<Pixels>,
    ) -> Option<ViewPoint> {
        let origin = self.scene_origin?;
        let screen = ViewPoint {
            x: (position.x - origin.x).as_f32(),
            y: (position.y - origin.y).as_f32(),
        };
        screen.is_finite().then_some(screen)
    }
}

fn sample_canvas_point(sample: ViewportPointerSample) -> Option<CanvasPoint> {
    let raw = sample.raw();
    sample.snapshot().transform().canvas_point_at_screen(raw)
}

fn canvas_rect_contains(bounds: CanvasRect, point: CanvasPoint) -> bool {
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y <= bounds.origin.y + bounds.size.height
}
