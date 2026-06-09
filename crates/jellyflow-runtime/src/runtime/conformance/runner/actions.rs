mod connection;
mod graph;
mod node;
mod rendering;
mod selection;
mod viewport;

use crate::runtime::store::NodeGraphStore;

use super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Result<(), String> {
    match action {
        ConformanceAction::DispatchTransaction { transaction } => {
            graph::dispatch_transaction(store, transaction)
        }
        ConformanceAction::ApplyNodeDrag { node, to } => node::apply_node_drag(store, *node, *to),
        ConformanceAction::ApplyNodeResize { request } => node::apply_node_resize(store, *request),
        ConformanceAction::ApplyNodePointerResize { request } => {
            node::apply_node_pointer_resize(store, *request)
        }
        ConformanceAction::ApplyNodePointerResizeSession { request } => {
            node::apply_node_pointer_resize_session(store, *request)
        }
        ConformanceAction::ApplyNodePointerDown { input } => {
            node::apply_node_pointer_down(store, *input);
            Ok(())
        }
        ConformanceAction::ApplySelectionBox { input } => {
            selection::apply_selection_box(store, *input);
            Ok(())
        }
        ConformanceAction::AssertConnectionTarget { input, expected } => {
            connection::assert_connection_target(*input, *expected)
        }
        ConformanceAction::AssertConnectionTargetFromHandles { input, expected } => {
            connection::assert_connection_target_from_handles(input, *expected)
        }
        ConformanceAction::ApplyConnectEdge { request } => {
            connection::apply_connect_edge(store, *request)
        }
        ConformanceAction::ApplyReconnectEdge { request } => {
            connection::apply_reconnect_edge(store, *request)
        }
        ConformanceAction::ApplyNodeNudge { request } => node::apply_node_nudge(store, *request),
        ConformanceAction::ApplyDeleteSelection => selection::apply_delete_selection(store),
        ConformanceAction::ApplyDeleteSelectionForKey { key } => {
            selection::apply_delete_selection_for_key(store, *key)
        }
        ConformanceAction::ApplyAutoPan { request } => viewport::apply_auto_pan(store, *request),
        ConformanceAction::ApplySelectionAutoPan { request } => {
            viewport::apply_selection_auto_pan(store, *request)
        }
        ConformanceAction::ApplyViewportPan { request } => viewport::apply_pan(store, *request),
        ConformanceAction::ApplyViewportPanConstrained {
            request,
            viewport_size,
        } => viewport::apply_pan_constrained(store, *request, *viewport_size),
        ConformanceAction::ApplyViewportZoom { request } => viewport::apply_zoom(store, *request),
        ConformanceAction::ApplyViewportZoomConstrained {
            request,
            viewport_size,
        } => viewport::apply_zoom_constrained(store, *request, *viewport_size),
        ConformanceAction::ApplyViewportAnimationFrame {
            request,
            elapsed_seconds,
        } => viewport::apply_animation_frame(store, *request, *elapsed_seconds),
        ConformanceAction::ApplyViewportAnimationFrames {
            request,
            elapsed_seconds,
        } => viewport::apply_animation_frames(store, *request, elapsed_seconds),
        ConformanceAction::AssertViewportAnimationFrame {
            request,
            elapsed_seconds,
            expected,
        } => viewport::assert_animation_frame(*request, *elapsed_seconds, *expected),
        ConformanceAction::ApplyViewportPanInertiaFrame {
            request,
            elapsed_seconds,
        } => viewport::apply_pan_inertia_frame(store, request, *elapsed_seconds),
        ConformanceAction::ApplyViewportPanInertiaFrames {
            request,
            elapsed_seconds,
        } => viewport::apply_pan_inertia_frames(store, request, elapsed_seconds),
        ConformanceAction::AssertViewportPanInertiaFrame {
            request,
            elapsed_seconds,
            expected,
        } => viewport::assert_pan_inertia_frame(request, *elapsed_seconds, *expected),
        ConformanceAction::ExpectViewportPanInertiaRejected { request } => {
            viewport::expect_pan_inertia_rejected(request)
        }
        ConformanceAction::AssertViewportDoubleClickZoom {
            input,
            expected,
            expect_rejection,
        } => viewport::assert_double_click_zoom(store, *input, *expected, *expect_rejection),
        ConformanceAction::ApplyViewportScrollGesture {
            context,
            input,
            expect_rejection,
        } => viewport::apply_scroll_gesture(store, *context, *input, *expect_rejection),
        ConformanceAction::ApplyViewportDragPanGesture {
            context,
            input,
            expect_rejection,
        } => viewport::apply_drag_pan_gesture(store, *context, *input, *expect_rejection),
        ConformanceAction::SetViewport { pan, zoom } => {
            graph::set_viewport(store, *pan, *zoom);
            Ok(())
        }
        ConformanceAction::AssertVisibleNodeIds {
            viewport_size,
            expected,
        } => rendering::assert_visible_node_ids(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleNodeRenderOrder {
            viewport_size,
            expected,
        } => rendering::assert_visible_node_render_order(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleEdgeIds {
            viewport_size,
            expected,
        } => rendering::assert_visible_edge_ids(store, *viewport_size, expected),
        ConformanceAction::AssertVisibleEdgeRenderOrder {
            viewport_size,
            expected,
        } => rendering::assert_visible_edge_render_order(store, *viewport_size, expected),
        ConformanceAction::SetSelection {
            nodes,
            edges,
            groups,
        } => {
            graph::set_selection(store, nodes, edges, groups);
            Ok(())
        }
        ConformanceAction::EmitGesture { event } => {
            graph::emit_gesture(store, event);
            Ok(())
        }
    }
}

fn require_commit<T, E: ToString>(
    result: Result<Option<T>, E>,
    action: &'static str,
) -> Result<(), String> {
    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(format!("{action} produced no commit")),
        Err(err) => Err(err.to_string()),
    }
}
