use std::borrow::Cow;
use std::rc::Rc;

use jellyflow_core::{CanvasPoint, Edge, EdgeId, EdgeKind, Graph, NodeId, PortId};
use jellyflow_runtime::geometry::{
    EdgePath, EdgePresentationFacts, HandlePosition, resolve_edge_route_path,
};
use jellyflow_runtime::schema::NodeCreation;
use jellyflow_runtime::{
    graph::CreateFromWireCandidates, graph::EdgeEndpoint, graph::EdgeInstanceKey,
    graph::PortConnectionDragIntent, interaction::CanvasInteractionError,
    interaction::CanvasInteractionPhase, interaction::CanvasInteractionPresentation,
    interaction::ConnectionGestureBinding, interaction::ConnectionInteractionStart,
    interaction::ConnectionProbeValidity, interaction::ConnectionSearchRequest,
    interaction::InteractionOwner, interaction::InteractionTerminalReason,
    interaction::InteractionTicketId, view::NodeInstanceKey, view::ViewCreateFromWireQueryError,
    viewport::ViewportPointerSample,
};
use open_gpui::{Context, Pixels, Window};

use crate::activity::OpenGpuiActivityKind;
use crate::binding::OpenGpuiMountCapability;
use crate::interaction::{
    NativeInteractionGeneration, OpenGpuiPortDragCancelReason, OpenGpuiPortDragOwner,
    OpenGpuiPortDragPhase, OpenGpuiPortEvent, OpenGpuiPortFocusExit,
    OpenGpuiSceneGestureCancelReason,
};
use crate::port_handle::{OpenGpuiHostPortHoverListener, OpenGpuiHostPortListener};
use crate::theme::OpenGpuiGraphStyleSlot;

use super::OpenGpuiSceneHost;
use super::endpoint_resolver::{
    ExplicitEndpointFallback, PublishedEndpointResolver, PublishedRouteEndpointIdentity,
    ResolvedEndpoint,
};
use super::interaction_session::NativeConnectionAdmission;
use super::lifecycle::NodeChanges;

pub(super) type OpenGpuiConnectionPreviewProbeOutcome = ConnectionProbeValidity;

pub(super) struct OpenGpuiConnectionPreviewPlan {
    pub(super) path: EdgePath,
    pub(super) edge_kind: EdgeKind,
    pub(super) style_slot: Option<OpenGpuiGraphStyleSlot>,
    pub(super) presentation: EdgePresentationFacts,
    pub(super) probe: OpenGpuiConnectionPreviewProbeOutcome,
    pub(super) suppressed_edge: Option<EdgeInstanceKey>,
}

/// Failure to operate on the application-owned native connection search surface.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiConnectionSearchError {
    #[error("connection search is no longer active")]
    InactiveRequest,
    #[error("connection search callback belongs to an older interaction generation")]
    StaleGeneration,
    #[error("connection search query failed")]
    Query(#[from] ViewCreateFromWireQueryError),
    #[error("connection search candidates belong to another source port")]
    CandidateSetMismatch,
    #[error("connection search candidate index is out of bounds: {index}")]
    CandidateUnavailable { index: usize },
    #[error("selected connection search candidate is no longer valid for the source port")]
    CandidateNoLongerValid,
    #[error("product creation does not match the selected semantic candidate")]
    CandidateCreationMismatch,
    #[error("the selected node kind no longer declares port {port:?}")]
    CandidatePortUnavailable { port: jellyflow_core::PortKey },
    #[error("selected connection search candidate produced no topology")]
    InvalidSelection,
    #[error("connection search interaction failed")]
    Interaction(#[from] CanvasInteractionError),
}

/// One create-from-wire selection accepted by the shared interaction session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiConnectionSearchSubmission {
    node: NodeId,
    ticket: InteractionTicketId,
}

impl OpenGpuiConnectionSearchSubmission {
    #[must_use]
    pub const fn node(&self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn ticket(&self) -> InteractionTicketId {
        self.ticket
    }
}

impl OpenGpuiSceneHost {
    /// Returns the owner of the current native port pointer capture, if any.
    #[must_use]
    pub fn active_port_drag(&self) -> Option<OpenGpuiPortDragOwner> {
        self.interaction_owner.active_port_owner()
    }

    /// Returns the renderer-neutral interaction presentation for native application UI.
    #[must_use]
    pub fn interaction_presentation(&self) -> CanvasInteractionPresentation {
        self.interaction_owner.presentation()
    }

    /// Returns the current create-from-wire request, including its interaction generation.
    #[must_use]
    pub fn connection_search_request(&self) -> Option<ConnectionSearchRequest> {
        if self.interaction_owner.presentation().phase() != CanvasInteractionPhase::ConnectionSearch
        {
            return None;
        }
        self.interaction_owner.search_request()
    }

    /// Queries semantic candidates and publishes the resulting search status to the session.
    pub fn connection_search_candidates(
        &mut self,
        request: &ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CreateFromWireCandidates, OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let result = self.interaction_owner.search_candidates(request);
        match result {
            Ok(candidates) => {
                let update = self
                    .interaction_owner
                    .search_ready(request, candidates.candidates().len())?;
                self.apply_canvas_interaction_update(update, window, cx);
                Ok(candidates)
            }
            Err(error) => {
                let update = self.interaction_owner.search_failed(request)?;
                self.apply_canvas_interaction_update(update, window, cx);
                Err(error.into())
            }
        }
    }

    /// Restarts application-owned candidate loading for the active search generation.
    pub fn retry_connection_search(
        &mut self,
        request: &ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let update = self.interaction_owner.retry_search(request)?;
        self.apply_canvas_interaction_update(update, window, cx);
        Ok(())
    }

    /// Cancels the active search without changing graph topology.
    pub fn cancel_connection_search(
        &mut self,
        request: &ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let update = self.interaction_owner.cancel()?;
        self.discard_pending_interaction_submission();
        self.apply_canvas_interaction_update(update, window, cx);
        Ok(())
    }

    /// Selects one candidate and retains its node creation and connection as one topology edit.
    pub fn select_connection_search_candidate(
        &mut self,
        request: &ConnectionSearchRequest,
        candidates: &CreateFromWireCandidates,
        candidate_index: usize,
        creation: NodeCreation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiConnectionSearchSubmission, OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let candidate = candidates
            .candidates()
            .get(candidate_index)
            .cloned()
            .ok_or(OpenGpuiConnectionSearchError::CandidateUnavailable {
                index: candidate_index,
            })?;
        let fresh_candidates = self.interaction_owner.search_candidates(request)?;
        let candidate = fresh_candidates
            .candidates()
            .iter()
            .find(|current| **current == candidate)
            .ok_or(OpenGpuiConnectionSearchError::CandidateNoLongerValid)?;
        if &creation.node().kind != candidate.node_kind()
            || creation.node().kind_version != candidate.kind_version()
        {
            return Err(OpenGpuiConnectionSearchError::CandidateCreationMismatch);
        }
        let node = creation.node_id();
        let port = creation.port_id(candidate.port()).ok_or_else(|| {
            OpenGpuiConnectionSearchError::CandidatePortUnavailable {
                port: candidate.port().clone(),
            }
        })?;
        let selection = self
            .interaction_owner
            .create_search_selection(request, port, creation)?
            .ok_or(OpenGpuiConnectionSearchError::InvalidSelection)?;
        let update = self.interaction_owner.select_search_result(selection)?;
        let ticket = self
            .interaction_owner
            .presentation()
            .connection()
            .and_then(|connection| connection.ticket())
            .ok_or(OpenGpuiConnectionSearchError::InvalidSelection)?;
        self.apply_canvas_interaction_update(update, window, cx);
        Ok(OpenGpuiConnectionSearchSubmission { node, ticket })
    }

    fn validate_connection_search_request(
        &self,
        request: &ConnectionSearchRequest,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        let Some(active) = self.connection_search_request() else {
            return Err(OpenGpuiConnectionSearchError::InactiveRequest);
        };
        if active.generation() != request.generation() {
            return Err(OpenGpuiConnectionSearchError::StaleGeneration);
        }
        if active != *request {
            return Err(OpenGpuiConnectionSearchError::InactiveRequest);
        }
        Ok(())
    }

    pub(super) fn cancel_connection_with_removed_source(&mut self, changes: &NodeChanges) {
        let Some((owner, mount)) = self.interaction_owner.connection_identity() else {
            return;
        };
        let instance = owner.instance();
        let source_is_current = changes.get(&instance.node()).is_none_or(|change| {
            if change.is_stale_for(instance)
                || !self
                    .mount_capability(instance)
                    .is_some_and(|current| current.is_same_mount(&mount))
            {
                return false;
            }
            matches!(
                self.view.port_instance_key(owner.port()),
                Ok(Some(current)) if current == owner.port_instance()
            )
        });
        if !source_is_current {
            self.retire_canvas_interaction_owner(InteractionOwner::Node(instance.lifetime()));
        }
    }

    pub(super) fn host_port_listener(
        cx: &Context<Self>,
        mount: OpenGpuiMountCapability,
    ) -> OpenGpuiHostPortListener {
        let host = cx.weak_entity();
        Rc::new(move |event, observer, window, cx| {
            let event = event.clone();
            let host = host.clone();
            let mount = mount.clone();
            if matches!(
                &event,
                OpenGpuiPortEvent::Pointer(event)
                    if matches!(
                        event.phase(),
                        OpenGpuiPortDragPhase::Start | OpenGpuiPortDragPhase::Move
                    )
            ) {
                let Some(host) = host.upgrade() else {
                    return;
                };
                let observed_event = event.clone();
                host.update(cx, |host, cx| {
                    host.handle_port_event_for_mount(event, &mount, window, cx);
                });
                if mount.is_live()
                    && let Some(observer) = observer
                {
                    observer(observed_event, window, cx);
                }
                return;
            }
            window.defer(cx, move |window, cx| {
                let Some(host) = host.upgrade() else {
                    return;
                };
                let observed_event = event.clone();
                host.update(cx, |host, cx| {
                    host.handle_port_event_for_mount(event, &mount, window, cx);
                });
                if mount.is_live()
                    && let Some(observer) = observer
                {
                    observer(observed_event, window, cx);
                }
            });
        })
    }

    pub(super) fn host_port_hover_listener(
        cx: &Context<Self>,
        mount: OpenGpuiMountCapability,
    ) -> OpenGpuiHostPortHoverListener {
        let host = cx.weak_entity();
        Rc::new(move |instance, port_instance, hovered, _window, cx| {
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                if host.interaction_owner.terminal() {
                    return;
                }
                let Some(owner) =
                    crate::interaction::port_drag_owner(instance.clone(), port_instance)
                else {
                    return;
                };
                if !host.port_event_owner_is_current(&owner, &mount) {
                    return;
                }
                let changed = if hovered {
                    host.retained_nodes
                        .set_hover(Some(instance.clone()), Some(port_instance))
                } else {
                    host.retained_nodes.clear_hovered_port(port_instance)
                };
                if changed {
                    cx.notify();
                }
            });
        })
    }

    #[cfg(test)]
    pub(super) fn handle_port_event(
        &mut self,
        event: OpenGpuiPortEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let instance = match &event {
            OpenGpuiPortEvent::Pointer(event) => event.owner().instance(),
            OpenGpuiPortEvent::Activation(event) => event.owner().instance(),
        };
        let Some(mount) = self.mount_capability(instance).cloned() else {
            return;
        };
        self.handle_port_event_for_mount(event, &mount, window, cx);
    }

    fn handle_port_event_for_mount(
        &mut self,
        event: OpenGpuiPortEvent,
        mount: &OpenGpuiMountCapability,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.interaction_owner.terminal() {
            return;
        }
        let owner = match &event {
            OpenGpuiPortEvent::Pointer(event) => event.owner(),
            OpenGpuiPortEvent::Activation(event) => event.owner(),
        };
        if !self.port_event_owner_is_current(owner, mount) {
            let retirement_cancellation = match &event {
                OpenGpuiPortEvent::Pointer(event) => match event.phase() {
                    OpenGpuiPortDragPhase::Cancel(
                        reason @ (OpenGpuiPortDragCancelReason::OwnerRetired
                        | OpenGpuiPortDragCancelReason::HostClosed),
                    ) => Some(reason),
                    _ => None,
                },
                OpenGpuiPortEvent::Activation(_) => None,
            };
            if let Some(reason) = retirement_cancellation
                && let OpenGpuiPortEvent::Pointer(event) = &event
                && self.interaction_owner.connection_matches_pointer(
                    owner,
                    mount,
                    event.native_generation(),
                )
            {
                self.cancel_connection_for_drag(reason, window, cx);
            }
            return;
        }

        match event {
            OpenGpuiPortEvent::Pointer(event) => match event.phase() {
                OpenGpuiPortDragPhase::Start
                    if self.navigation.input_bindings().connects_on_click()
                        && self
                            .interaction_owner
                            .keyboard_connection_source()
                            .is_some() =>
                {
                    self.reject_native_connection_start(event.owner(), mount, window, cx);
                }
                OpenGpuiPortDragPhase::Start => match event.native_generation() {
                    Some(native_generation) => self.begin_connection_drag(
                        event.owner(),
                        mount,
                        native_generation,
                        event.position(),
                        window,
                        cx,
                    ),
                    None => self.reject_native_connection_start(event.owner(), mount, window, cx),
                },
                OpenGpuiPortDragPhase::Move => self.move_connection_drag(
                    event.owner(),
                    mount,
                    event.native_generation(),
                    event.position(),
                    window,
                    cx,
                ),
                OpenGpuiPortDragPhase::Release
                    if self.navigation.input_bindings().connects_on_click()
                        && self
                            .interaction_owner
                            .keyboard_connection_source()
                            .is_some() =>
                {
                    self.activate_port(event.owner(), mount, window, cx);
                }
                OpenGpuiPortDragPhase::Release => self.release_connection_drag(
                    event.owner(),
                    mount,
                    event.native_generation(),
                    event.position(),
                    window,
                    cx,
                ),
                OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape)
                    if self.navigation.input_bindings().connects_on_click()
                        && self
                            .interaction_owner
                            .keyboard_connection_source()
                            .is_some() =>
                {
                    self.cancel_connection_for_drag(
                        OpenGpuiPortDragCancelReason::Escape,
                        window,
                        cx,
                    );
                }
                OpenGpuiPortDragPhase::Cancel(
                    OpenGpuiPortDragCancelReason::InteractionUnavailable
                    | OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded,
                ) => {}
                OpenGpuiPortDragPhase::Cancel(reason)
                    if self.interaction_owner.connection_matches_pointer(
                        event.owner(),
                        mount,
                        event.native_generation(),
                    ) =>
                {
                    self.cancel_connection_for_drag(reason, window, cx);
                }
                OpenGpuiPortDragPhase::Cancel(_) => {}
            },
            OpenGpuiPortEvent::Activation(event) => {
                self.activate_port(event.owner(), mount, window, cx);
            }
        }
    }

    fn port_event_owner_is_current(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
    ) -> bool {
        let instance = owner.instance();
        if !mount.is_live()
            || self.retained_nodes.retained_instance(instance.node()) != Some(instance)
            || !self
                .mount_capability(instance)
                .is_some_and(|current| current.is_same_mount(mount))
        {
            return false;
        }
        if instance.view() != self.view.id()
            || !matches!(
                self.view.node_lifetime_key(instance.node()),
                Ok(Some(current)) if current == instance.lifetime()
            )
        {
            return false;
        }
        matches!(
            self.view.port_instance_key(owner.port()),
            Ok(Some(current)) if current == owner.port_instance()
        )
    }

    fn activate_port(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(port) = self.view.keyboard_connection_port(owner.port_instance()) else {
            return;
        };
        let Some(admission) = NativeConnectionAdmission::activation(owner.clone(), mount.clone())
        else {
            return;
        };
        let result = self.interaction_owner.activate_connection_port(
            port,
            EdgeId::new(),
            self.connection_options.mode,
            admission,
        );
        match result {
            Ok(Some(update)) => self.apply_canvas_interaction_update(update, window, cx),
            Ok(None) => {}
            Err(error) => self.record_command_diagnostic(error.to_string()),
        }
    }

    fn begin_connection_drag(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        native_generation: NativeInteractionGeneration,
        position: open_gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = owner.port();
        let Ok(query) = self.view.port_connection_drag_intent(owner.port_instance()) else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        #[cfg(test)]
        self.scene_evidence
            .last_port_connection_query_work
            .set(Some(query.work()));
        let Some(drag_intent) = query.into_outcome().ready() else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        let sample = self.viewport_pointer_sample(position).or_else(|| {
            self.view
                .geometry_snapshot()
                .ok()
                .and_then(|geometry| geometry.port(source).map(|port| port.route_anchor()))
                .and_then(|anchor| self.interaction_sample_for_canvas(anchor))
        });
        let (Some(sample), Some(start)) = (sample, self.connection_start(owner, drag_intent))
        else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        let Some(admission) =
            NativeConnectionAdmission::pointer(owner.clone(), mount.clone(), native_generation)
        else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        let Ok(Some(update)) = self
            .interaction_owner
            .press_connection(start, sample, admission)
        else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        self.apply_canvas_interaction_update(update, window, cx);
    }

    fn reject_native_connection_start(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interaction_owner.reject_port_start(
            owner,
            mount,
            OpenGpuiPortDragCancelReason::InteractionUnavailable,
            window,
            cx,
        );
    }

    fn move_connection_drag(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        native_generation: Option<NativeInteractionGeneration>,
        position: open_gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .interaction_owner
            .connection_matches_pointer(owner, mount, native_generation)
        {
            return;
        }
        let Some(sample) = self.viewport_pointer_sample(position) else {
            return;
        };
        if let Ok(update) = self.interaction_owner.move_to(sample) {
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    fn release_connection_drag(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        native_generation: Option<NativeInteractionGeneration>,
        position: open_gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .interaction_owner
            .connection_matches_pointer(owner, mount, native_generation)
        {
            return;
        }
        let Some(sample) = self.viewport_pointer_sample(position) else {
            self.cancel_connection_for_drag(
                OpenGpuiPortDragCancelReason::InteractionUnavailable,
                window,
                cx,
            );
            return;
        };
        if self.navigation.input_bindings().connects_on_click() {
            let Ok(source) = self.view.keyboard_connection_port(owner.port_instance()) else {
                self.cancel_connection_for_drag(
                    OpenGpuiPortDragCancelReason::InteractionUnavailable,
                    window,
                    cx,
                );
                return;
            };
            let update = self.interaction_owner.release_connection_or_activate_port(
                source,
                EdgeId::new(),
                self.connection_options.mode,
                sample,
            );
            self.apply_canvas_interaction_result(update, window, cx);
            return;
        }
        if let Ok(update) = self.interaction_owner.release(sample) {
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    fn connection_start(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        drag_intent: PortConnectionDragIntent,
    ) -> Option<ConnectionInteractionStart> {
        let source = owner.port();
        if drag_intent.port() != source
            || drag_intent.node() != owner.instance().node()
            || drag_intent.instance() != owner.port_instance()
            || !drag_intent.can_begin_connection()
        {
            return None;
        }
        let options = self.connection_options;
        let mode = options.mode;
        let new_edge = EdgeId::new();
        let binding =
            ConnectionInteractionStart::binding_from_port_drag(drag_intent, new_edge, mode)?;
        let geometry = self.view.geometry_snapshot().ok()?;
        let snapshot = self.view.graph_snapshot().ok()?;
        let resolver = PublishedEndpointResolver::new(&geometry);
        let fixed = self.resolve_binding_port_endpoint(
            &resolver,
            snapshot.graph(),
            &binding,
            binding.fixed_port_instance().port(),
        )?;
        let diagnostic = fixed.diagnostic();
        let fixed = fixed.position();
        let moving_outward = if let Some(detached) = binding.detached_port_instance() {
            self.resolve_binding_port_endpoint(
                &resolver,
                snapshot.graph(),
                &binding,
                detached.port(),
            )?
            .outward()
        } else {
            opposite_handle_position(fixed.position)
        };
        if let Some(diagnostic) = diagnostic {
            self.record_endpoint_geometry_diagnostics(vec![diagnostic.to_string()], 1);
        }
        self.view
            .connection_start_from_port_drag_intent(
                drag_intent,
                new_edge,
                mode,
                fixed.point,
                fixed.position,
                moving_outward,
                options.allow_create_from_wire,
            )
            .ok()
            .flatten()
    }

    fn resolve_binding_port_endpoint(
        &self,
        resolver: &PublishedEndpointResolver<'_>,
        graph: &Graph,
        binding: &ConnectionGestureBinding,
        port: PortId,
    ) -> Option<ResolvedEndpoint> {
        let route = binding_route_endpoint(binding, port);
        let fallback = self.explicit_endpoint_fallback(resolver, graph, port);
        resolver.resolve_port(port, route, fallback)
    }

    pub(super) fn explicit_endpoint_fallback(
        &self,
        resolver: &PublishedEndpointResolver<'_>,
        graph: &Graph,
        port: PortId,
    ) -> Option<ExplicitEndpointFallback> {
        let port = graph.port(&port)?;
        let node = graph.node(&port.node)?;
        let bounds = resolver
            .geometry()
            .node(port.node)
            .filter(|geometry| geometry.is_fresh())
            .map(|geometry| geometry.bounds())
            .or_else(|| {
                let size = node.size?;
                node.bounds_for_size(size)
            })?;
        resolver
            .explicit_node_box_fallback(bounds, HandlePosition::fallback_for_direction(port.dir))
    }

    pub(super) fn interaction_sample_for_canvas(
        &self,
        canvas: CanvasPoint,
    ) -> Option<ViewportPointerSample> {
        let transform = self.view.viewport_transform().ok()?;
        let view = transform.screen_point_for_canvas(canvas)?;
        self.view.viewport_pointer_sample(view).ok()
    }

    pub(super) fn cancel_connection_for_drag(
        &mut self,
        reason: OpenGpuiPortDragCancelReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((owner, _)) = self.interaction_owner.connection_identity() else {
            return;
        };
        let update = if reason == OpenGpuiPortDragCancelReason::OwnerRetired {
            Ok(self
                .interaction_owner
                .owner_retired(InteractionOwner::Node(owner.instance().lifetime())))
        } else {
            self.interaction_owner.cancel()
        };
        self.discard_pending_interaction_submission();
        self.apply_canvas_interaction_result(update, window, cx);
    }

    pub(super) fn retire_connection_context(
        &mut self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) {
        if let Some((owner, current_mount)) = self.interaction_owner.connection_identity()
            && owner.instance() == instance
            && current_mount.is_same_mount(mount)
        {
            self.interaction_owner.cancel_port_owner_deferred(
                &owner,
                mount,
                OpenGpuiPortDragCancelReason::OwnerRetired,
            );
        }
    }

    pub(super) fn sync_native_connection_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(owner) = self.stale_connection_context_owner() {
            self.interaction_owner
                .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
            if let Some((current_owner, current_mount)) =
                self.interaction_owner.connection_identity()
            {
                self.interaction_owner.cancel_port_owner_deferred(
                    &current_owner,
                    &current_mount,
                    OpenGpuiPortDragCancelReason::OwnerRetired,
                );
            }
            self.retire_canvas_interaction_owner(owner);
            cx.notify();
        }

        let presentation = self.interaction_owner.presentation();
        let keyboard_connection = self
            .interaction_owner
            .keyboard_connection_source()
            .is_some();
        let native_armed = presentation.phase() == CanvasInteractionPhase::Armed
            && self
                .interaction_owner
                .connection_identity()
                .is_some_and(|(owner, _)| {
                    self.interaction_owner
                        .uses_owner(InteractionOwner::Port(owner.port_instance()))
                });
        if presentation.connection().is_none() && !native_armed && !keyboard_connection {
            if let Some((owner, mount)) = self.interaction_owner.connection_identity() {
                let (exit, cancellation) = match presentation.terminal() {
                    Some(InteractionTerminalReason::OwnerRetired) => (
                        OpenGpuiPortFocusExit::OwnerRetired,
                        OpenGpuiPortDragCancelReason::OwnerRetired,
                    ),
                    Some(InteractionTerminalReason::ViewClosed) => (
                        OpenGpuiPortFocusExit::HostClosed,
                        OpenGpuiPortDragCancelReason::HostClosed,
                    ),
                    _ => (
                        OpenGpuiPortFocusExit::Restore,
                        OpenGpuiPortDragCancelReason::InteractionUnavailable,
                    ),
                };
                self.interaction_owner
                    .cancel_port_owner_deferred(&owner, &mount, cancellation);
                self.interaction_owner
                    .settle_port_focus(&owner, &mount, exit, window, cx);
            }
            self.interaction_owner.clear_connection();
            return;
        }
        if presentation.connection().is_none() && !keyboard_connection {
            return;
        }

        let owner_identity = self.interaction_owner.connection_identity();
        let owner_is_current = owner_identity
            .as_ref()
            .is_some_and(|(owner, mount)| self.port_event_owner_is_current(owner, mount));
        if !owner_is_current {
            let retired_node = owner_identity.map(|(owner, _)| owner.instance().lifetime());
            let update = match retired_node {
                Some(node) => Ok(self
                    .interaction_owner
                    .owner_retired(InteractionOwner::Node(node))),
                None => self.interaction_owner.cancel(),
            };
            self.discard_pending_interaction_submission();
            self.apply_canvas_interaction_result(update, window, cx);
            return;
        }

        if self.interaction_owner.connection_retains_activity() {
            return;
        }

        let owner_data = self
            .interaction_owner
            .connection_identity()
            .map(|(owner, mount)| {
                let instance = owner.instance().clone();
                (owner, mount, instance)
            });
        let lease = owner_data.as_ref().and_then(|(_, mount, instance)| {
            self.retained_nodes.activity_for_mount(instance, mount)
        });
        let lease = lease.and_then(|activity| {
            activity
                .acquire(OpenGpuiActivityKind::GraphInteraction)
                .ok()
        });
        let retained = match (owner_data, lease) {
            (Some((owner, mount, _)), Some(lease)) => self
                .interaction_owner
                .retain_connection_activity(&owner, &mount, lease),
            _ => false,
        };
        if !retained {
            let update = self.interaction_owner.cancel();
            self.discard_pending_interaction_submission();
            self.apply_canvas_interaction_result(update, window, cx);
        }
    }

    fn stale_connection_context_owner(&self) -> Option<InteractionOwner> {
        let binding = self
            .interaction_owner
            .presentation()
            .connection()?
            .binding();
        if self.interaction_owner.connection_matches_binding(&binding)
            && self.view.validate_connection_binding(&binding).is_ok()
        {
            return None;
        }
        if let Some(edge) = binding.reconnect_edge() {
            Some(InteractionOwner::Edge(edge))
        } else {
            Some(InteractionOwner::Port(binding.fixed_port_instance()))
        }
    }

    pub(super) fn connection_preview_plan(&mut self) -> Option<OpenGpuiConnectionPreviewPlan> {
        let presentation = self.interaction_owner.presentation();
        let connection = presentation.connection()?;
        let wire = connection.wire();
        let snapshot = self.view.graph_snapshot().ok()?;
        let graph = snapshot.graph();
        let binding = connection.binding();
        if !self.interaction_owner.connection_matches_binding(&binding)
            || self.view.validate_connection_binding(&binding).is_err()
        {
            return None;
        }
        let style_slot = graph
            .port(&binding.fixed_port_instance().port())?
            .ty
            .as_ref()
            .and_then(|ty| self.edge_painters.style_slot_for_type(ty));
        if let Some(suppressed) = wire.suppressed_edge()
            && snapshot.edge_instance_key(suppressed.edge()) != Some(suppressed)
        {
            return None;
        }

        let preview_edge = match binding.reconnect_edge() {
            None => {
                let source = graph.port(&binding.fixed_port_instance().port())?;
                Cow::Owned(Edge::new(
                    source.kind.edge_kind(),
                    binding.fixed_port_instance().port(),
                    binding.fixed_port_instance().port(),
                ))
            }
            Some(edge) => Cow::Borrowed(graph.edge(&edge.edge())?),
        };
        let route = resolve_edge_route_path(preview_edge.as_ref(), wire.endpoints())?;
        Some(OpenGpuiConnectionPreviewPlan {
            path: route.path,
            edge_kind: route.edge_kind,
            style_slot,
            presentation: route.presentation,
            probe: wire.probe_validity(),
            suppressed_edge: wire.suppressed_edge(),
        })
    }
}

fn binding_route_endpoint(
    binding: &ConnectionGestureBinding,
    port: PortId,
) -> Option<PublishedRouteEndpointIdentity> {
    let edge = binding.reconnect_edge()?;
    let fixed_endpoint = binding.fixed_endpoint();
    if binding
        .detached_port_instance()
        .is_some_and(|detached| detached.port() == port)
    {
        return Some(PublishedRouteEndpointIdentity::new(
            edge.edge(),
            opposite_edge_endpoint(fixed_endpoint),
        ));
    }
    if binding.fixed_port_instance().port() == port {
        return Some(PublishedRouteEndpointIdentity::new(
            edge.edge(),
            fixed_endpoint,
        ));
    }
    None
}

pub(super) fn opposite_edge_endpoint(endpoint: EdgeEndpoint) -> EdgeEndpoint {
    match endpoint {
        EdgeEndpoint::From => EdgeEndpoint::To,
        EdgeEndpoint::To => EdgeEndpoint::From,
    }
}

fn opposite_handle_position(position: HandlePosition) -> HandlePosition {
    match position {
        HandlePosition::Top => HandlePosition::Bottom,
        HandlePosition::Right => HandlePosition::Left,
        HandlePosition::Bottom => HandlePosition::Top,
        HandlePosition::Left => HandlePosition::Right,
    }
}
