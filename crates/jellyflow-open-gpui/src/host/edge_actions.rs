use jellyflow::core::{EdgeId, NodeId};
use jellyflow::runtime::runtime::geometry::EdgePathAnchor;
use jellyflow::runtime::{
    CanvasPressTarget, ConnectionInteractionStart, EdgeEndpoint, InteractionPosition,
    PublishedEdgeRoute, SelectionMode, ViewPoint,
};
use open_gpui::{Context, KeyDownEvent, Window};

use crate::interaction::port_drag_owner;

use super::OpenGpuiSceneHost;
use super::connection::MountedConnectionOwner;
use super::focus_navigation::OpenGpuiGraphFocusTarget;

impl OpenGpuiSceneHost {
    pub(super) fn activate_node_action(
        &mut self,
        node: NodeId,
        selection_mode: SelectionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let selectable = self
            .view
            .graph_snapshot()
            .ok()
            .and_then(|snapshot| {
                snapshot
                    .graph()
                    .node(&node)
                    .map(|node| node.selectable.unwrap_or(true))
            })
            .unwrap_or(false);
        if !selectable {
            return false;
        }
        match self.canvas_interaction.activate_node(node, selection_mode) {
            Ok(update) => {
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                false
            }
        }
    }

    pub(super) fn activate_edge_action(
        &mut self,
        edge: EdgeId,
        selection_mode: SelectionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let position = self.edge_interaction_position(edge);
        let Some(position) = position else {
            return false;
        };
        let pressed = self.canvas_interaction.press(
            CanvasPressTarget::Edge {
                edge,
                selection_mode,
            },
            position,
        );
        let Ok(pressed) = pressed else {
            return false;
        };
        self.apply_canvas_interaction_update(pressed, window, cx);
        let released = self.canvas_interaction.release(position);
        let Ok(released) = released else {
            return false;
        };
        self.apply_canvas_interaction_update(released, window, cx);
        true
    }

    pub(super) fn handle_graph_root_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.modified()
            || !self
                .interaction_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
        {
            return false;
        }
        if event.keystroke.key.eq_ignore_ascii_case("f") {
            return match self.fit_view(cx) {
                Ok(changed) => changed,
                Err(error) => {
                    self.record_command_diagnostic(error.to_string());
                    false
                }
            };
        }
        if !event.keystroke.key.eq_ignore_ascii_case("delete")
            && !event.keystroke.key.eq_ignore_ascii_case("backspace")
        {
            return false;
        }
        let edge = self
            .view
            .selection()
            .ok()
            .and_then(|selection| selection.edges().last().copied());
        edge.is_some_and(|edge| self.delete_edge_action(edge, window, cx))
    }

    pub(super) fn delete_edge_action(
        &mut self,
        edge: EdgeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let geometry = match self.view.geometry_snapshot() {
            Ok(geometry) => geometry,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                return false;
            }
        };
        let Some(route) = geometry.route(edge) else {
            return false;
        };
        let update = self.canvas_interaction.delete_edge(route);
        match update {
            Ok(update) if !update.effects().is_empty() => {
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Ok(_) => false,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                false
            }
        }
    }

    pub(super) fn edge_reconnect_press_target(
        &mut self,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
    ) -> Option<CanvasPressTarget> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let route = geometry.route(edge)?;
        if !endpoint_reconnect_allowed(route, endpoint) {
            return None;
        }
        let snapshot = self.view.graph_snapshot().ok()?;
        let options = self.connection_options;
        let start = ConnectionInteractionStart::reconnect_from_graph_route(
            snapshot.graph(),
            route,
            endpoint,
            options.mode,
            options.allow_create_from_wire,
        )?;
        let owner_node = start.owner_node();
        let detached_port = start.intent().detached_port()?;
        let instance = self.mounts.retained_instance(owner_node)?.clone();
        let mount = self.mount_capability(&instance)?.clone();
        self.connection_owner = Some(MountedConnectionOwner::new(
            port_drag_owner(instance, detached_port),
            mount,
        ));
        Some(CanvasPressTarget::Connection(start))
    }

    pub(super) fn edge_reconnect_is_current(&self, edge: EdgeId, endpoint: EdgeEndpoint) -> bool {
        self.view
            .geometry_snapshot()
            .ok()
            .and_then(|geometry| geometry.route(edge).cloned())
            .is_some_and(|route| endpoint_reconnect_allowed(&route, endpoint))
    }

    pub(super) fn activate_edge_reconnect_action(
        &mut self,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let position = self.edge_endpoint_interaction_position(edge, endpoint);
        let Some(position) = position else {
            return false;
        };
        let Some(CanvasPressTarget::Connection(start)) =
            self.edge_reconnect_press_target(edge, endpoint)
        else {
            return false;
        };
        match self
            .canvas_interaction
            .activate_connection_search(start, position)
        {
            Ok(update) => {
                self.graph_focus
                    .remember_return_target(OpenGpuiGraphFocusTarget::reconnect(edge, endpoint));
                if let Some(focus) = &self.interaction_focus {
                    window.focus(focus, cx);
                }
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Err(error) => {
                self.connection_owner = None;
                self.record_command_diagnostic(error.to_string());
                false
            }
        }
    }

    fn edge_interaction_position(&self, edge: EdgeId) -> Option<InteractionPosition> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let route = geometry.route(edge)?;
        let sample = route.facts().path.sample(EdgePathAnchor::Center).ok()?;
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        let view = transform.screen_point_for_canvas(sample.point)?;
        InteractionPosition::new(ViewPoint::new(view.x, view.y), sample.point)
    }

    fn edge_endpoint_interaction_position(
        &self,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
    ) -> Option<InteractionPosition> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let route = geometry.route(edge)?;
        let anchor = match endpoint {
            EdgeEndpoint::From => EdgePathAnchor::Source,
            EdgeEndpoint::To => EdgePathAnchor::Target,
        };
        let sample = route.facts().path.sample(anchor).ok()?;
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        let view = transform.screen_point_for_canvas(sample.point)?;
        InteractionPosition::new(ViewPoint::new(view.x, view.y), sample.point)
    }
}

fn endpoint_reconnect_allowed(route: &PublishedEdgeRoute, endpoint: EdgeEndpoint) -> bool {
    match endpoint {
        EdgeEndpoint::From => route.facts().interaction.reconnect_source,
        EdgeEndpoint::To => route.facts().interaction.reconnect_target,
    }
}
