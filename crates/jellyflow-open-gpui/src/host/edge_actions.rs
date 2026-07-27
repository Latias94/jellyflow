use jellyflow_core::{EdgeId, Graph, PortId};
use jellyflow_runtime::geometry::EdgePathAnchor;
use jellyflow_runtime::{
    graph::EdgeEndpoint, graph::EdgeInstanceKey, graph::NodeLifetimeKey,
    interaction::CanvasInteractionError, interaction::CanvasInteractionPhase,
    interaction::CanvasPressTarget, interaction::ConnectionInteractionStart,
    interaction::InteractionOwner, interaction::NodeNudgeDirection, interaction::SelectionMode,
    view::GeometrySnapshot, view::PublishedEdgeRoute, viewport::ViewportPointerSample,
};
use open_gpui::{Context, KeyDownEvent, KeyUpEvent, Window};

use crate::interaction::port_drag_owner;

use super::OpenGpuiSceneHost;
use super::connection::opposite_edge_endpoint;
use super::endpoint_resolver::{
    PublishedEndpointResolver, PublishedRouteEndpointIdentity, ResolvedEndpoint,
};
use super::focus_navigation::OpenGpuiGraphFocusTarget;
use super::interaction_session::NativeConnectionAdmission;

#[derive(Debug, Clone, Copy)]
struct ResolvedReconnectEndpoints {
    detached: ResolvedEndpoint,
    fixed: ResolvedEndpoint,
}

pub(super) struct ResolvedEdgeReconnectPress {
    start: ConnectionInteractionStart,
    admission: NativeConnectionAdmission,
}

impl ResolvedEdgeReconnectPress {
    pub(super) fn into_parts(self) -> (ConnectionInteractionStart, NativeConnectionAdmission) {
        (self.start, self.admission)
    }
}

impl OpenGpuiSceneHost {
    pub(super) fn activate_node_action(
        &mut self,
        node: NodeLifetimeKey,
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
                    .node(&node.node())
                    .map(|node| node.selectable.unwrap_or(true))
            })
            .unwrap_or(false);
        if !selectable {
            return false;
        }
        match self.interaction_owner.activate_node(node, selection_mode) {
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
        instance: EdgeInstanceKey,
        selection_mode: SelectionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.edge_instance_is_current(instance) {
            return false;
        }
        let edge = instance.edge();
        let sample = self.edge_interaction_sample(edge);
        let Some(sample) = sample else {
            return false;
        };
        let pressed = self.interaction_owner.press(
            CanvasPressTarget::Edge {
                instance,
                selection_mode,
            },
            sample,
        );
        let pressed = match pressed {
            Ok(pressed) => pressed,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                return false;
            }
        };
        self.apply_canvas_interaction_update(pressed, window, cx);
        let released = self.interaction_owner.release(sample);
        let released = match released {
            Ok(released) => released,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                match self.interaction_owner.cancel() {
                    Ok(cancelled) => self.apply_canvas_interaction_update(cancelled, window, cx),
                    Err(cancel_error) => {
                        self.record_command_diagnostic(cancel_error.to_string());
                    }
                }
                return false;
            }
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
        let root_focused = self
            .interaction_owner
            .focus_handle()
            .is_some_and(|focus| focus.is_focused(window));
        let exact_graph_target = self.interaction_owner.exact_focused_graph_target(window);
        if !root_focused && exact_graph_target.is_none() {
            self.space_pan_held = false;
            return false;
        }
        if is_unmodified_tab(event) {
            return self.reveal_focused_node_action(window, cx);
        }
        if !event.keystroke.modifiers.modified()
            && event.keystroke.key.eq_ignore_ascii_case("escape")
            && self
                .interaction_owner
                .keyboard_connection_source()
                .is_some()
        {
            let update = self.interaction_owner.cancel();
            self.discard_pending_interaction_submission();
            self.apply_canvas_interaction_result(update, window, cx);
            return true;
        }
        if root_focused
            && event.keystroke.key.eq_ignore_ascii_case("space")
            && !event.keystroke.modifiers.modified()
            && self.navigation.input_bindings().space_left_pans()
        {
            let changed = !self.space_pan_held;
            self.space_pan_held = true;
            if changed {
                cx.notify();
            }
            return true;
        }
        if event.keystroke.modifiers.modified() && !only_shift_is_modified(event) {
            return false;
        }
        if (root_focused
            || matches!(
                exact_graph_target,
                Some(OpenGpuiGraphFocusTarget::Node { .. })
            ))
            && let Some(direction) = node_nudge_direction(event)
        {
            return self.nudge_selection_action(
                direction,
                event.keystroke.modifiers.shift,
                window,
                cx,
            );
        }
        if root_focused && event.keystroke.key.eq_ignore_ascii_case("f") {
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
        if event.is_held
            || self.interaction_owner.presentation().phase() != CanvasInteractionPhase::Idle
        {
            return false;
        }
        self.delete_selection_action(window, cx)
    }

    pub(super) fn handle_graph_root_key_up(
        &mut self,
        event: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !event.keystroke.modifiers.modified()
            && (event.keystroke.key.eq_ignore_ascii_case("enter")
                || event.keystroke.key.eq_ignore_ascii_case("space"))
            && let Some(OpenGpuiGraphFocusTarget::Edge(edge)) =
                self.interaction_owner.exact_focused_graph_target(window)
        {
            return self.activate_edge_action(edge, SelectionMode::Replace, window, cx);
        }
        if !event.keystroke.key.eq_ignore_ascii_case("space") {
            return false;
        }
        let was_held = std::mem::take(&mut self.space_pan_held);
        if was_held {
            cx.notify();
        }
        was_held
            && self
                .interaction_owner
                .focus_handle()
                .is_some_and(|focus| focus.is_focused(window))
    }

    fn delete_selection_action(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.interaction_owner.delete_selection() {
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

    fn nudge_selection_action(
        &mut self,
        direction: NodeNudgeDirection,
        accelerated: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self
            .interaction_owner
            .nudge_selection(direction, accelerated)
        {
            Ok(update) if update.effects().is_empty() => false,
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

    fn reveal_focused_node_action(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(OpenGpuiGraphFocusTarget::Node { lifetime, .. }) =
            self.interaction_owner.exact_focused_graph_target(window)
        else {
            return false;
        };
        let Some(viewport) = self.surface_size else {
            return false;
        };
        let padding = self
            .navigation
            .fit_view_padding_in_view()
            .min(viewport.width.min(viewport.height) * 0.25);
        match self
            .interaction_owner
            .reveal_node(lifetime, viewport, padding)
        {
            Ok(update) if update.damage().viewport => {
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Ok(_) => false,
            Err(CanvasInteractionError::AlreadyActive) => false,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                false
            }
        }
    }

    pub(super) fn edge_reconnect_press_target(
        &self,
        instance: EdgeInstanceKey,
        endpoint: EdgeEndpoint,
    ) -> Option<ResolvedEdgeReconnectPress> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let edge = instance.edge();
        let route = geometry.route(edge)?;
        if route.instance() != instance {
            return None;
        }
        if !endpoint_reconnect_allowed(route, endpoint) {
            return None;
        }
        let snapshot = self.view.graph_snapshot().ok()?;
        let resolved =
            self.resolve_reconnect_endpoints(&geometry, snapshot.graph(), edge, endpoint)?;
        let options = self.connection_options;
        let start = self
            .view
            .connection_start_from_route(
                route,
                endpoint,
                resolved.fixed.position(),
                options.mode,
                options.allow_create_from_wire,
            )
            .ok()
            .flatten()?;
        let owner_node = start.owner_node();
        let binding = start.binding();
        let detached_instance = binding.detached_port_instance()?;
        let node_instance = self.mounts.retained_instance(owner_node)?.clone();
        let mount = self.mount_capability(&node_instance)?.clone();
        let owner = port_drag_owner(node_instance, detached_instance)?;
        let admission = NativeConnectionAdmission::activation(owner, mount)?;
        Some(ResolvedEdgeReconnectPress { start, admission })
    }

    pub(super) fn edge_reconnect_is_current(
        &self,
        instance: EdgeInstanceKey,
        endpoint: EdgeEndpoint,
    ) -> bool {
        let presentation = self.interaction_owner.presentation();
        let Some(snapshot) = self.view.graph_snapshot().ok() else {
            return false;
        };
        if snapshot.edge_instance_key(instance.edge()) != Some(instance) {
            return false;
        }
        if presentation.phase() == CanvasInteractionPhase::Armed {
            return self
                .interaction_owner
                .uses_owner(InteractionOwner::Edge(instance));
        }
        let Some(connection) = presentation.connection() else {
            return false;
        };
        let binding = connection.binding();
        if !self.interaction_owner.connection_matches_binding(&binding)
            || self.view.validate_connection_binding(&binding).is_err()
        {
            return false;
        }
        let Some(active_edge) = binding.reconnect_edge() else {
            return false;
        };
        let active_endpoint = opposite_edge_endpoint(binding.fixed_endpoint());
        if active_edge != instance || active_endpoint != endpoint {
            return false;
        }

        true
    }

    pub(super) fn activate_edge_reconnect_action(
        &mut self,
        instance: EdgeInstanceKey,
        endpoint: EdgeEndpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.edge_instance_is_current(instance) {
            return false;
        }
        let edge = instance.edge();
        let sample = self.edge_endpoint_interaction_sample(edge, endpoint);
        let Some(sample) = sample else {
            return false;
        };
        let Some(plan) = self.edge_reconnect_press_target(instance, endpoint) else {
            return false;
        };
        let (start, admission) = plan.into_parts();
        match self
            .interaction_owner
            .activate_connection(start, sample, admission)
        {
            Ok(Some(update)) => {
                self.interaction_owner.remember_graph_focus_return(
                    OpenGpuiGraphFocusTarget::reconnect(instance, endpoint),
                );
                if let Some(focus) = self.interaction_owner.focus_handle() {
                    window.focus(&focus, cx);
                }
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Ok(None) => false,
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                false
            }
        }
    }

    fn edge_instance_is_current(&self, instance: EdgeInstanceKey) -> bool {
        matches!(
            self.view.edge_instance_key(instance.edge()),
            Ok(Some(current)) if current == instance
        )
    }

    fn edge_interaction_sample(&self, edge: EdgeId) -> Option<ViewportPointerSample> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let route = geometry.route(edge)?;
        let sample = route.facts().path.sample(EdgePathAnchor::Center).ok()?;
        self.interaction_sample_for_canvas(sample.point)
    }

    fn edge_endpoint_interaction_sample(
        &self,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
    ) -> Option<ViewportPointerSample> {
        let geometry = self.view.geometry_snapshot().ok()?;
        let snapshot = self.view.graph_snapshot().ok()?;
        let resolved =
            self.resolve_reconnect_endpoints(&geometry, snapshot.graph(), edge, endpoint)?;
        self.interaction_sample_for_canvas(resolved.detached.point())
    }

    fn resolve_reconnect_endpoints(
        &self,
        geometry: &GeometrySnapshot,
        graph: &Graph,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
    ) -> Option<ResolvedReconnectEndpoints> {
        let graph_edge = graph.edge(&edge)?;
        let (detached_port, fixed_port) = match endpoint {
            EdgeEndpoint::From => (graph_edge.from, graph_edge.to),
            EdgeEndpoint::To => (graph_edge.to, graph_edge.from),
        };
        let fixed_endpoint = opposite_edge_endpoint(endpoint);
        let resolver = PublishedEndpointResolver::new(geometry);
        let detached =
            self.resolve_reconnect_endpoint(&resolver, graph, edge, endpoint, detached_port)?;
        let fixed =
            self.resolve_reconnect_endpoint(&resolver, graph, edge, fixed_endpoint, fixed_port)?;
        Some(ResolvedReconnectEndpoints { detached, fixed })
    }

    fn resolve_reconnect_endpoint(
        &self,
        resolver: &PublishedEndpointResolver<'_>,
        graph: &Graph,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        port: PortId,
    ) -> Option<ResolvedEndpoint> {
        let fallback = self.explicit_endpoint_fallback(resolver, graph, port);
        resolver.resolve_route(
            port,
            PublishedRouteEndpointIdentity::new(edge, endpoint),
            fallback,
        )
    }
}

fn node_nudge_direction(event: &KeyDownEvent) -> Option<NodeNudgeDirection> {
    match event.keystroke.key.as_str() {
        key if key.eq_ignore_ascii_case("left") => Some(NodeNudgeDirection::Left),
        key if key.eq_ignore_ascii_case("right") => Some(NodeNudgeDirection::Right),
        key if key.eq_ignore_ascii_case("up") => Some(NodeNudgeDirection::Up),
        key if key.eq_ignore_ascii_case("down") => Some(NodeNudgeDirection::Down),
        _ => None,
    }
}

fn only_shift_is_modified(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    modifiers.shift
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
}

fn is_unmodified_tab(event: &KeyDownEvent) -> bool {
    let modifiers = event.keystroke.modifiers;
    event.keystroke.key.eq_ignore_ascii_case("tab")
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
        && !modifiers.function
}

fn endpoint_reconnect_allowed(route: &PublishedEdgeRoute, endpoint: EdgeEndpoint) -> bool {
    match endpoint {
        EdgeEndpoint::From => route.facts().interaction.reconnect_source,
        EdgeEndpoint::To => route.facts().interaction.reconnect_target,
    }
}
