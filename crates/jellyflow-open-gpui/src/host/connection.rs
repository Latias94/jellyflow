use std::rc::Rc;

use jellyflow::core::{CanvasPoint, EdgeId, NodeId, PortDirection};
use jellyflow::runtime::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointPosition, EdgePath, EdgePosition, HandlePosition,
    bezier_edge_path, handle_anchor_position, resolve_edge_route_path,
};
use jellyflow::runtime::schema::NodeCatalogInstantiationError;
use jellyflow::runtime::{
    CanvasInteractionError, CanvasInteractionPhase, CanvasInteractionPresentation,
    CanvasPressTarget, ConnectionGestureIntent, ConnectionInteractionStart, ConnectionProbe,
    ConnectionSearchRequest, ConnectionSearchSelection, CreateFromWireCandidates, EdgeEndpoint,
    GeometrySnapshot, InteractionOwner, InteractionPosition, InteractionTerminalReason,
    InteractionTicketId, NodeInstanceKey, PortConnectionDragIntent, ViewCreateFromWireQueryError,
    ViewPoint,
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

use super::OpenGpuiSceneHost;
use super::focus_navigation::OpenGpuiGraphFocusTarget;
use super::lifecycle::NodeChanges;

pub(super) struct OpenGpuiConnectionPreviewPlan {
    pub(super) path: EdgePath,
    pub(super) suppressed_edge: Option<EdgeId>,
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
    #[error("selected node kind could not be instantiated")]
    Instantiation(#[from] NodeCatalogInstantiationError),
    #[error("the selected node kind no longer declares port {port:?}")]
    CandidatePortUnavailable { port: jellyflow::core::PortKey },
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
        request: ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<CreateFromWireCandidates, OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let result = self
            .view
            .create_from_wire_candidates(request.source(), request.mode());
        match result {
            Ok(candidates) => {
                let update = self
                    .interaction_owner
                    .search_ready(request.generation(), candidates.candidates().len())?;
                self.apply_canvas_interaction_update(update, window, cx);
                Ok(candidates)
            }
            Err(error) => {
                let update = self.interaction_owner.search_failed(request.generation())?;
                self.apply_canvas_interaction_update(update, window, cx);
                Err(error.into())
            }
        }
    }

    /// Restarts application-owned candidate loading for the active search generation.
    pub fn retry_connection_search(
        &mut self,
        request: ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let update = self.interaction_owner.retry_search(request.generation())?;
        self.apply_canvas_interaction_update(update, window, cx);
        Ok(())
    }

    /// Cancels the active search without changing graph topology.
    pub fn cancel_connection_search(
        &mut self,
        request: ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        let update = self.interaction_owner.cancel();
        self.discard_pending_interaction_submission();
        self.apply_canvas_interaction_update(update, window, cx);
        Ok(())
    }

    /// Selects one candidate and retains its node creation and connection as one topology edit.
    pub fn select_connection_search_candidate(
        &mut self,
        request: ConnectionSearchRequest,
        candidates: &CreateFromWireCandidates,
        candidate_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiConnectionSearchSubmission, OpenGpuiConnectionSearchError> {
        self.validate_connection_search_request(request)?;
        if candidates.source() != request.source() {
            return Err(OpenGpuiConnectionSearchError::CandidateSetMismatch);
        }
        let candidate = candidates
            .candidates()
            .get(candidate_index)
            .cloned()
            .ok_or(OpenGpuiConnectionSearchError::CandidateUnavailable {
                index: candidate_index,
            })?;
        let fresh_candidates = self
            .view
            .create_from_wire_candidates(request.source(), request.mode())?;
        let candidate = fresh_candidates
            .candidates()
            .iter()
            .find(|current| **current == candidate)
            .ok_or(OpenGpuiConnectionSearchError::CandidateNoLongerValid)?;
        let creation = self
            .catalog
            .instantiate_node(candidate.node_kind(), request.position().canvas())?;
        let node = creation.node_id();
        let port = creation.port_id(candidate.port()).ok_or_else(|| {
            OpenGpuiConnectionSearchError::CandidatePortUnavailable {
                port: candidate.port().clone(),
            }
        })?;
        let selection = ConnectionSearchSelection::new(
            port,
            fresh_candidates.revision(),
            creation.into_topology_edit(),
        )
        .ok_or(OpenGpuiConnectionSearchError::InvalidSelection)?;
        let update = self
            .interaction_owner
            .select_search_result(request.generation(), selection)?;
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
        request: ConnectionSearchRequest,
    ) -> Result<(), OpenGpuiConnectionSearchError> {
        let Some(active) = self.connection_search_request() else {
            return Err(OpenGpuiConnectionSearchError::InactiveRequest);
        };
        if active.generation() != request.generation() {
            return Err(OpenGpuiConnectionSearchError::StaleGeneration);
        }
        if active != request {
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
            self.view
                .graph_snapshot()
                .ok()
                .and_then(|snapshot| snapshot.graph().port(&owner.port()).map(|port| port.node))
                == Some(instance.node())
        });
        if !source_is_current {
            self.retire_canvas_interaction_owner(InteractionOwner::Node(instance.node()));
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
        Rc::new(move |instance, port, hovered, _window, cx| {
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                if host.interaction_owner.terminal() {
                    return;
                }
                let owner = crate::interaction::port_drag_owner(instance.clone(), port);
                if !host.port_event_owner_is_current(&owner, &mount) {
                    return;
                }
                let changed = if hovered {
                    let changed = host.hovered_node != Some(instance.node())
                        || host.hovered_port != Some(port);
                    host.hovered_node = Some(instance.node());
                    host.hovered_port = Some(port);
                    changed
                } else if host.hovered_port == Some(port) {
                    host.hovered_port = None;
                    true
                } else {
                    false
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
                OpenGpuiPortDragPhase::Release => self.release_connection_drag(
                    event.owner(),
                    mount,
                    event.native_generation(),
                    event.position(),
                    window,
                    cx,
                ),
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
            || self.mounts.retained_instance(instance.node()) != Some(instance)
            || !self
                .mount_capability(instance)
                .is_some_and(|current| current.is_same_mount(mount))
        {
            return false;
        }
        if !matches!(
            self.view.node_incarnation(instance.node()),
            Ok(Some(current)) if current == instance.incarnation()
        ) {
            return false;
        }
        self.view
            .graph_snapshot()
            .ok()
            .and_then(|snapshot| snapshot.graph().port(&owner.port()).map(|port| port.node))
            == Some(instance.node())
    }

    fn activate_port(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = owner.port();
        let Ok(Some(drag_intent)) = self.view.port_connection_drag_intent(source) else {
            return;
        };
        let Some(source_anchor) = self
            .view
            .geometry_snapshot()
            .ok()
            .and_then(|geometry| geometry.port(source).map(|port| port.route_anchor()))
        else {
            return;
        };
        let Some(position) = self.interaction_position_for_canvas(source_anchor) else {
            return;
        };
        let Some(start) = self.connection_start(owner, drag_intent) else {
            return;
        };
        let Ok(update) = self
            .interaction_owner
            .activate_connection_search(start, position)
        else {
            return;
        };
        self.interaction_owner
            .begin_activation_connection(owner.clone(), mount.clone());
        self.interaction_owner
            .remember_graph_focus_return(OpenGpuiGraphFocusTarget::Port {
                node: owner.instance().node(),
                port: owner.port(),
            });
        self.apply_canvas_interaction_update(update, window, cx);
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
        let Ok(Some(drag_intent)) = self.view.port_connection_drag_intent(source) else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        let position = self.interaction_position(position).or_else(|| {
            self.view
                .geometry_snapshot()
                .ok()
                .and_then(|geometry| geometry.port(source).map(|port| port.route_anchor()))
                .and_then(|anchor| self.interaction_position_for_canvas(anchor))
        });
        let (Some(position), Some(start)) = (position, self.connection_start(owner, drag_intent))
        else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        let Ok(update) = self
            .interaction_owner
            .press(CanvasPressTarget::Connection(start), position)
        else {
            self.reject_native_connection_start(owner, mount, window, cx);
            return;
        };
        self.interaction_owner.begin_pointer_connection(
            owner.clone(),
            mount.clone(),
            native_generation,
        );
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
        let Some(position) = self.interaction_position(position) else {
            return;
        };
        if let Ok(update) = self.interaction_owner.move_to(position) {
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
        let Some(position) = self.interaction_position(position) else {
            self.cancel_connection_for_drag(
                OpenGpuiPortDragCancelReason::InteractionUnavailable,
                window,
                cx,
            );
            return;
        };
        if self.interaction_owner.presentation().phase() == CanvasInteractionPhase::Armed {
            let Ok(moved) = self.interaction_owner.move_to(position) else {
                self.cancel_connection_for_drag(
                    OpenGpuiPortDragCancelReason::InteractionUnavailable,
                    window,
                    cx,
                );
                return;
            };
            self.apply_canvas_interaction_update(moved, window, cx);
        }
        if let Ok(update) = self.interaction_owner.release(position) {
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    fn connection_start(
        &self,
        owner: &OpenGpuiPortDragOwner,
        drag_intent: PortConnectionDragIntent,
    ) -> Option<ConnectionInteractionStart> {
        let source = owner.port();
        if drag_intent.port() != source
            || drag_intent.node() != owner.instance().node()
            || !drag_intent.can_begin_connection()
        {
            return None;
        }
        let options = self.connection_options;
        let mode = options.mode;
        let new_edge = EdgeId::new();
        let intent =
            ConnectionInteractionStart::intent_from_port_drag(drag_intent, new_edge, mode)?;
        let geometry = self.view.geometry_snapshot().ok()?;
        let fixed = self.fixed_connection_endpoint(&geometry, intent)?;
        ConnectionInteractionStart::from_port_drag_intent(
            drag_intent,
            new_edge,
            mode,
            fixed.point,
            fixed.position,
            options.allow_create_from_wire,
        )
    }

    fn fixed_connection_endpoint(
        &self,
        geometry: &GeometrySnapshot,
        intent: ConnectionGestureIntent,
    ) -> Option<EdgeEndpointPosition> {
        if let Some(port) = geometry.port(intent.fixed_port()) {
            return Some(EdgeEndpointPosition {
                point: port.route_anchor(),
                position: port.outward(),
            });
        }

        if let ConnectionGestureIntent::Reconnect { edge, endpoint, .. } = intent
            && let Some(route) = geometry.route(edge)
        {
            let endpoints = route.facts().endpoints;
            return Some(match endpoint {
                EdgeEndpoint::From => endpoints.target,
                EdgeEndpoint::To => endpoints.source,
            });
        }

        let snapshot = self.view.graph_snapshot().ok()?;
        let graph = snapshot.graph();
        let port = graph.port(&intent.fixed_port())?;
        let node = graph.node(&port.node)?;
        let bounds = geometry
            .node(port.node)
            .filter(|geometry| geometry.is_fresh())
            .map(|geometry| geometry.bounds())
            .or_else(|| {
                let size = node.size.or_else(|| {
                    self.catalog
                        .get(&node.kind)
                        .and_then(|spec| spec.default_size())
                })?;
                node.bounds_for_size(size)
            })?;
        handle_anchor_position(
            bounds,
            None,
            HandlePosition::fallback_for_direction(port.dir),
        )
    }

    fn interaction_position_for_canvas(&self, canvas: CanvasPoint) -> Option<InteractionPosition> {
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        let view = transform.screen_point_for_canvas(canvas)?;
        InteractionPosition::from_view(ViewPoint::new(view.x, view.y), transform)
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
            self.interaction_owner
                .owner_retired(InteractionOwner::Node(owner.instance().node()))
        } else {
            self.interaction_owner.cancel()
        };
        self.discard_pending_interaction_submission();
        self.apply_canvas_interaction_update(update, window, cx);
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
        let native_armed = presentation.phase() == CanvasInteractionPhase::Armed
            && self
                .interaction_owner
                .connection_identity()
                .is_some_and(|(owner, _)| {
                    self.interaction_owner
                        .uses_owner(InteractionOwner::Port(owner.port()))
                });
        if presentation.connection().is_none() && !native_armed {
            if let Some((owner, mount)) = self.interaction_owner.connection_identity() {
                let exit = match presentation.terminal() {
                    Some(InteractionTerminalReason::OwnerRetired) => {
                        OpenGpuiPortFocusExit::OwnerRetired
                    }
                    Some(InteractionTerminalReason::ViewClosed) => {
                        OpenGpuiPortFocusExit::HostClosed
                    }
                    _ => OpenGpuiPortFocusExit::Restore,
                };
                self.interaction_owner
                    .settle_port_focus(&owner, &mount, exit, window, cx);
            }
            self.interaction_owner.clear_connection();
            return;
        }
        if presentation.connection().is_none() {
            return;
        }

        let owner_identity = self.interaction_owner.connection_identity();
        let owner_is_current = owner_identity.as_ref().is_some_and(|(owner, mount)| {
            mount.is_live()
                && self
                    .mount_capability(owner.instance())
                    .is_some_and(|current| current.is_same_mount(mount))
        });
        if !owner_is_current {
            let retired_node = owner_identity.map(|(owner, _)| owner.instance().node());
            let update = match retired_node {
                Some(node) => self
                    .interaction_owner
                    .owner_retired(InteractionOwner::Node(node)),
                None => self.interaction_owner.cancel(),
            };
            self.discard_pending_interaction_submission();
            self.apply_canvas_interaction_update(update, window, cx);
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
            self.mounts.get(instance).and_then(|mounted| {
                mounted
                    .mount
                    .is_same_mount(mount)
                    .then(|| mounted.activity.clone())
            })
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
            self.apply_canvas_interaction_update(update, window, cx);
        }
    }

    fn stale_connection_context_owner(&self) -> Option<InteractionOwner> {
        let intent = self.interaction_owner.presentation().connection()?.intent();
        let snapshot = self.view.graph_snapshot().ok()?;
        let graph = snapshot.graph();
        match intent {
            ConnectionGestureIntent::Connect { source, .. } => graph
                .port(&source)
                .is_none()
                .then_some(InteractionOwner::Port(source)),
            ConnectionGestureIntent::Reconnect {
                edge,
                endpoint,
                detached_port,
                fixed_port,
                ..
            } => {
                let Some(current) = graph.edge(&edge) else {
                    return Some(InteractionOwner::Edge(edge));
                };
                let endpoints_match = match endpoint {
                    EdgeEndpoint::From => current.from == detached_port && current.to == fixed_port,
                    EdgeEndpoint::To => current.to == detached_port && current.from == fixed_port,
                };
                (!endpoints_match
                    || graph.port(&detached_port).is_none()
                    || graph.port(&fixed_port).is_none())
                .then_some(InteractionOwner::Edge(edge))
            }
        }
    }

    pub(super) fn connection_preview_plan(
        &self,
        geometry: &GeometrySnapshot,
    ) -> Option<OpenGpuiConnectionPreviewPlan> {
        let presentation = self.interaction_owner.presentation();
        let connection = presentation.connection()?;
        let snapshot = self.view.graph_snapshot().ok()?;
        let graph = snapshot.graph();
        let intent = connection.intent();
        let fixed = geometry
            .port(intent.fixed_port())
            .map(|port| EdgeEndpointPosition {
                point: port.route_anchor(),
                position: port.outward(),
            })
            .unwrap_or(EdgeEndpointPosition {
                point: connection.fixed_anchor(),
                position: connection.fixed_outward(),
            });
        let moving = match connection.probe() {
            ConnectionProbe::Valid { target, .. } => geometry
                .port(target.port())
                .map(|port| EdgeEndpointPosition {
                    point: port.route_anchor(),
                    position: port.outward(),
                })
                .unwrap_or(EdgeEndpointPosition {
                    point: target.route_anchor(),
                    position: target.outward(),
                }),
            ConnectionProbe::None | ConnectionProbe::Invalid { .. } => EdgeEndpointPosition {
                point: connection.pointer().canvas(),
                position: intent
                    .detached_port()
                    .and_then(|port| geometry.port(port).map(|port| port.outward()))
                    .unwrap_or_else(|| opposite_handle_position(fixed.position)),
            },
        };

        let path = match intent {
            ConnectionGestureIntent::Connect { source, .. } => {
                let endpoints = match graph.port(&source)?.dir {
                    PortDirection::Out => EdgePosition {
                        source: fixed,
                        target: moving,
                    },
                    PortDirection::In => EdgePosition {
                        source: moving,
                        target: fixed,
                    },
                };
                bezier_edge_path(
                    endpoints.source,
                    endpoints.target,
                    BezierEdgeOptions::default(),
                )?
            }
            ConnectionGestureIntent::Reconnect { edge, endpoint, .. } => {
                let endpoints = match endpoint {
                    EdgeEndpoint::From => EdgePosition {
                        source: moving,
                        target: fixed,
                    },
                    EdgeEndpoint::To => EdgePosition {
                        source: fixed,
                        target: moving,
                    },
                };
                resolve_edge_route_path(graph.edge(&edge)?, endpoints)?.path
            }
        };
        Some(OpenGpuiConnectionPreviewPlan {
            path,
            suppressed_edge: connection.suppressed_edge(),
        })
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
