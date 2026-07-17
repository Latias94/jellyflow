use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, EdgeId, NodeGraphConnectionMode, NodeId, PortDirection, PortId,
};
use jellyflow::runtime::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointPosition, EdgePath, EdgePosition, HandlePosition,
    bezier_edge_path, resolve_edge_route_path,
};
use jellyflow::runtime::schema::{CreateFromWireCandidate, NodeCatalogInstantiationError};
use jellyflow::runtime::{
    ConnectionDecision, ConnectionEvaluation, CreateFromWireCandidates, GeometrySnapshot,
    GraphCommandConflict, GraphCommandNoop, GraphCommandOutcome, GraphCommandReceipt,
    GraphCommandRejection, GraphRevision, NodeInstanceKey, PortConnectionDragIntent,
    ViewCreateFromWireQueryError, ViewSessionError,
};
use open_gpui::{Context, Pixels, Window};

use crate::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease};
use crate::binding::OpenGpuiMountCapability;
use crate::command_journal::{
    OpenGpuiCommandEnqueueError, OpenGpuiCommandOrigin, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketError,
};
use crate::connection_controller::{
    OpenGpuiConnectionCancelReason, OpenGpuiConnectionCommitRejection,
    OpenGpuiConnectionGeneration, OpenGpuiConnectionIntent, OpenGpuiConnectionPhase,
    OpenGpuiConnectionPoint, OpenGpuiConnectionTransition, OpenGpuiCreateFromWireRequest,
};
use crate::interaction::{
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragOwner, OpenGpuiPortDragPhase, OpenGpuiPortEvent,
};
use crate::port_handle::OpenGpuiHostPortListener;
use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::OpenGpuiSceneHost;
use super::lifecycle::NodeChanges;

pub(super) struct OpenGpuiConnectionPreview {
    pub(super) owner: OpenGpuiPortDragOwner,
    pub(super) mount: OpenGpuiMountCapability,
    pub(super) generation: OpenGpuiConnectionGeneration,
    pub(super) intent: OpenGpuiConnectionIntent,
    pub(super) pointer: CanvasPoint,
}

pub(super) struct OpenGpuiConnectionPreviewPlan {
    pub(super) path: EdgePath,
    pub(super) suppressed_edge: Option<EdgeId>,
}

/// Revision-bound search results for one active create-from-wire request.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiCreateFromWireCandidates {
    request: OpenGpuiCreateFromWireRequest,
    inner: CreateFromWireCandidates,
}

impl OpenGpuiCreateFromWireCandidates {
    #[must_use]
    pub const fn request(&self) -> OpenGpuiCreateFromWireRequest {
        self.request
    }

    #[must_use]
    pub const fn revision(&self) -> GraphRevision {
        self.inner.revision()
    }

    #[must_use]
    pub fn candidates(&self) -> &[CreateFromWireCandidate] {
        self.inner.candidates()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Structured create-from-wire failure retained by the native search surface.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiCreateFromWireError {
    #[error("create-from-wire request is no longer active")]
    InactiveRequest,
    #[error("create-from-wire callback belongs to an older interaction generation")]
    StaleGeneration,
    #[error("create-from-wire query failed")]
    Query(#[from] ViewCreateFromWireQueryError),
    #[error("create-from-wire candidate index is out of bounds: {index}")]
    CandidateUnavailable { index: usize },
    #[error("selected node kind could not be instantiated")]
    Instantiation(#[from] NodeCatalogInstantiationError),
    #[error("the selected node kind no longer declares port {port:?}")]
    CandidatePortUnavailable { port: jellyflow::core::PortKey },
    #[error("create-from-wire commit preparation failed")]
    CommitPreparation(#[from] crate::connection_controller::OpenGpuiConnectionCommitRejection),
    #[error("create-from-wire command could not be executed")]
    Session(#[from] ViewSessionError),
    #[error(
        "create-from-wire query revision {query_revision} is stale; current revision is {current_revision}"
    )]
    StaleQuery {
        query_revision: GraphRevision,
        current_revision: GraphRevision,
    },
    #[error(transparent)]
    CommandEnqueue(#[from] OpenGpuiCommandEnqueueError),
}

/// One enqueued create-from-wire command and its preallocated node identity.
#[derive(Debug)]
pub struct OpenGpuiCreateFromWireSubmission {
    node: NodeId,
    ticket: OpenGpuiCommandTicket,
}

impl OpenGpuiCreateFromWireSubmission {
    #[must_use]
    pub const fn node(&self) -> NodeId {
        self.node
    }

    pub fn into_ticket(self) -> OpenGpuiCommandTicket {
        self.ticket
    }
}

/// Runtime operation that surfaced a session error during a native port commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiConnectionCommitStage {
    Evaluation,
    Execution,
}

/// Lossless reason why a native port commit did not mutate the graph.
#[derive(Debug)]
pub enum OpenGpuiConnectionCommitFailure {
    Preparation(OpenGpuiConnectionCommitRejection),
    PolicyRejected(ConnectionEvaluation),
    Conflict(GraphCommandConflict),
    Rejected(GraphCommandRejection),
    Session {
        stage: OpenGpuiConnectionCommitStage,
        error: ViewSessionError,
    },
    CommandEnqueue(OpenGpuiCommandEnqueueError),
    Command(OpenGpuiCommandTicketError),
}

/// Result of one ordinary port-to-port commit attempt.
#[derive(Debug)]
pub enum OpenGpuiConnectionCommitOutcome {
    Applied(GraphCommandReceipt),
    Noop(GraphCommandNoop),
    Failed(OpenGpuiConnectionCommitFailure),
}

/// Correlated native port commit result retained after the controller returns to idle.
#[derive(Debug)]
pub struct OpenGpuiConnectionCommitEvent {
    pub(super) generation: OpenGpuiConnectionGeneration,
    target: PortId,
    outcome: OpenGpuiConnectionCommitOutcome,
}

impl OpenGpuiConnectionCommitEvent {
    #[must_use]
    pub const fn generation(&self) -> OpenGpuiConnectionGeneration {
        self.generation
    }

    #[must_use]
    pub const fn target(&self) -> PortId {
        self.target
    }

    #[must_use]
    pub const fn outcome(&self) -> &OpenGpuiConnectionCommitOutcome {
        &self.outcome
    }

    #[must_use]
    pub fn into_outcome(self) -> OpenGpuiConnectionCommitOutcome {
        self.outcome
    }
}

pub(super) struct MountedConnectionOwner {
    pub(super) owner: OpenGpuiPortDragOwner,
    pub(super) mount: OpenGpuiMountCapability,
    search_lease: Option<OpenGpuiActivityLease>,
}

impl MountedConnectionOwner {
    fn matches(&self, owner: &OpenGpuiPortDragOwner, mount: &OpenGpuiMountCapability) -> bool {
        self.owner == *owner && self.mount.is_same_mount(mount)
    }

    fn retain_search(&mut self, lease: OpenGpuiActivityLease) {
        self.search_lease = Some(lease);
    }

    fn retains_search(&self) -> bool {
        self.search_lease.is_some()
    }
}

impl OpenGpuiSceneHost {
    /// Returns the graph-semantic owner of the current native port capture, if any.
    #[must_use]
    pub fn active_port_drag(&self) -> Option<OpenGpuiPortDragOwner> {
        self.interactions.active_owner()
    }

    /// Current graph-semantic connection phase owned by this host.
    #[must_use]
    pub fn connection_phase(&self) -> OpenGpuiConnectionPhase {
        self.connections.phase()
    }

    /// Latest inert request for an application-owned create-from-wire search surface.
    #[must_use]
    pub fn create_from_wire_request(&self) -> Option<OpenGpuiCreateFromWireRequest> {
        self.last_create_from_wire
    }

    /// Returns the most recent ordinary port commit result, including structured failures.
    #[must_use]
    pub fn last_connection_commit(&self) -> Option<&OpenGpuiConnectionCommitEvent> {
        self.last_connection_commit.as_ref()
    }

    /// Takes the most recent ordinary port commit result for application-owned reconciliation.
    pub fn take_last_connection_commit(&mut self) -> Option<OpenGpuiConnectionCommitEvent> {
        self.last_connection_commit.take()
    }

    /// Queries the runtime's semantic catalog for the current native search request.
    ///
    /// Query errors leave the search active so the application can show a retry action. Empty
    /// results are returned explicitly and remain application-owned until cancelled.
    pub fn create_from_wire_candidates(
        &self,
        request: OpenGpuiCreateFromWireRequest,
    ) -> Result<OpenGpuiCreateFromWireCandidates, OpenGpuiCreateFromWireError> {
        self.validate_create_from_wire_request(request)?;
        let query = self
            .view
            .create_from_wire_candidates(request.source(), request.mode())?;
        Ok(OpenGpuiCreateFromWireCandidates {
            request,
            inner: query,
        })
    }

    /// Projects the request's canvas position into this host's local logical pixels.
    ///
    /// Native applications can use this as an overlay anchor without learning the view transform.
    /// `None` means the scene has not completed its first native measurement yet.
    pub fn create_from_wire_local_position(
        &self,
        request: OpenGpuiCreateFromWireRequest,
    ) -> Result<Option<OpenGpuiConnectionPoint>, OpenGpuiCreateFromWireError> {
        self.validate_create_from_wire_request(request)?;
        let Some(transform) = self.scene_transform else {
            return Ok(None);
        };
        let position = request.position();
        Ok(transform
            .screen_point_for_canvas(CanvasPoint {
                x: position.x(),
                y: position.y(),
            })
            .map(|point| OpenGpuiConnectionPoint::new(point.x, point.y)))
    }

    /// Cancels the current native search without changing graph topology.
    pub fn cancel_create_from_wire(
        &mut self,
        request: OpenGpuiCreateFromWireRequest,
        reason: OpenGpuiConnectionCancelReason,
    ) -> Result<(), OpenGpuiCreateFromWireError> {
        self.validate_create_from_wire_request(request)?;
        self.abort_connection(request.generation(), reason);
        Ok(())
    }

    /// Enqueues one revision-bound node creation and connection as an atomic topology command.
    pub fn commit_create_from_wire(
        &mut self,
        query: &OpenGpuiCreateFromWireCandidates,
        candidate_index: usize,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiCreateFromWireSubmission, OpenGpuiCreateFromWireError> {
        self.validate_create_from_wire_request(query.request)?;
        let current_revision = self.view.graph_snapshot()?.revision();
        if query.inner.revision() != current_revision {
            return Err(OpenGpuiCreateFromWireError::StaleQuery {
                query_revision: query.inner.revision(),
                current_revision,
            });
        }
        let candidate = query.inner.candidates().get(candidate_index).ok_or(
            OpenGpuiCreateFromWireError::CandidateUnavailable {
                index: candidate_index,
            },
        )?;
        let position = query.request.position();
        let creation = self.catalog.instantiate_node(
            candidate.node_kind(),
            CanvasPoint {
                x: position.x(),
                y: position.y(),
            },
        )?;
        let node = creation.node_id();
        let port = creation.port_id(candidate.port()).ok_or_else(|| {
            OpenGpuiCreateFromWireError::CandidatePortUnavailable {
                port: candidate.port().clone(),
            }
        })?;
        let commit = self.connections.commit_create_from_wire(
            query.request.generation(),
            port,
            query.inner.revision(),
        )?;
        let command = commit.into_topology_command(creation.into_topology_edit());
        let ticket = match self.commands.enqueue(
            command,
            OpenGpuiCommandOrigin::CreateFromWire {
                generation: query.request.generation(),
            },
        ) {
            Ok(ticket) => ticket,
            Err(error) => {
                let _ = self
                    .connections
                    .reject_create_from_wire_commit(query.request.generation());
                return Err(error.into());
            }
        };
        cx.notify();
        Ok(OpenGpuiCreateFromWireSubmission { node, ticket })
    }

    fn validate_create_from_wire_request(
        &self,
        request: OpenGpuiCreateFromWireRequest,
    ) -> Result<(), OpenGpuiCreateFromWireError> {
        let Some(active) = self.last_create_from_wire else {
            return Err(OpenGpuiCreateFromWireError::InactiveRequest);
        };
        if active.generation() != request.generation() {
            return Err(OpenGpuiCreateFromWireError::StaleGeneration);
        }
        if active != request || self.connections.phase() != OpenGpuiConnectionPhase::Searching {
            return Err(OpenGpuiCreateFromWireError::InactiveRequest);
        }
        if !self
            .connection_owner
            .as_ref()
            .is_some_and(MountedConnectionOwner::retains_search)
        {
            return Err(OpenGpuiCreateFromWireError::InactiveRequest);
        }
        Ok(())
    }

    pub(super) fn cancel_connection_with_removed_source(&mut self, changes: &NodeChanges) {
        let source_is_current = self.connection_owner.as_ref().is_none_or(|current| {
            let instance = current.owner.instance();
            let Some(change) = changes.get(&instance.node()) else {
                return true;
            };
            if change.is_stale_for(instance)
                || !self
                    .mount_capability(instance)
                    .is_some_and(|mount| mount.is_same_mount(&current.mount))
            {
                return true;
            }
            self.view.graph_snapshot().ok().and_then(|snapshot| {
                snapshot
                    .graph()
                    .port(&current.owner.port())
                    .map(|port| port.node)
            }) == Some(instance.node())
        });
        if !source_is_current {
            self.cancel_connection_for_drag(OpenGpuiPortDragCancelReason::PortDisabled);
        }
    }

    pub(super) fn host_port_listener(
        cx: &Context<Self>,
        mount: OpenGpuiMountCapability,
    ) -> OpenGpuiHostPortListener {
        let host = cx.weak_entity();
        Rc::new(move |event, window, cx| {
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
                host.update(cx, |host, cx| {
                    host.handle_port_event_for_mount(event, &mount, window, cx);
                });
                return;
            }
            window.defer(cx, move |window, cx| {
                let Some(host) = host.upgrade() else {
                    return;
                };
                host.update(cx, |host, cx| {
                    host.handle_port_event_for_mount(event, &mount, window, cx);
                });
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            let Some(reason) = retirement_cancellation else {
                return;
            };
            if self
                .connection_owner
                .as_ref()
                .is_some_and(|current| current.matches(owner, mount))
            {
                self.cancel_connection_for_drag(reason);
            }
            cx.notify();
            return;
        }

        match event {
            OpenGpuiPortEvent::Pointer(event) => match event.phase() {
                OpenGpuiPortDragPhase::Start => {
                    self.begin_connection_drag(event.owner(), mount, event.position());
                }
                OpenGpuiPortDragPhase::Move => {
                    self.update_connection_preview(event.owner(), mount, event.position());
                }
                OpenGpuiPortDragPhase::Release => {
                    self.update_connection_preview(event.owner(), mount, event.position());
                    self.finish_connection_drag(event.position(), cx);
                }
                OpenGpuiPortDragPhase::Cancel(reason) => self.cancel_connection_for_drag(reason),
            },
            OpenGpuiPortEvent::Activation(event) => {
                self.activate_port(event.owner(), mount);
            }
        }
        cx.notify();
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

    fn activate_port(&mut self, owner: &OpenGpuiPortDragOwner, mount: &OpenGpuiMountCapability) {
        let port = owner.port();
        let Ok(Some(drag_intent)) = self.view.port_connection_drag_intent(port) else {
            return;
        };
        let Some(position) = self
            .view
            .geometry_snapshot()
            .ok()
            .and_then(|geometry| geometry.port(port).map(|port| port.route_anchor()))
        else {
            return;
        };
        let Some(generation) =
            self.begin_connection_drag_with_intent(owner, mount, drag_intent, position)
        else {
            return;
        };
        self.request_create_from_wire_search(generation, position);
    }

    fn begin_connection_drag(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        position: open_gpui::Point<Pixels>,
    ) {
        let source = owner.port();
        let Ok(Some(drag_intent)) = self.view.port_connection_drag_intent(source) else {
            return;
        };
        let pointer = self.canvas_position(position).or_else(|| {
            self.view
                .geometry_snapshot()
                .ok()
                .and_then(|geometry| geometry.port(source).map(|port| port.route_anchor()))
        });
        if let Some(pointer) = pointer {
            self.begin_connection_drag_with_intent(owner, mount, drag_intent, pointer);
        }
    }

    fn begin_connection_drag_with_intent(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        drag_intent: PortConnectionDragIntent,
        pointer: CanvasPoint,
    ) -> Option<OpenGpuiConnectionGeneration> {
        let source = owner.port();
        if drag_intent.port() != source
            || !drag_intent.can_begin_connection()
            || !pointer.is_finite()
        {
            return None;
        }
        let mode = self.connection_mode();
        let intent = match drag_intent {
            PortConnectionDragIntent::Connect { .. } => {
                OpenGpuiConnectionIntent::connect(EdgeId::new(), source, mode)
            }
            PortConnectionDragIntent::Reconnect { candidate, .. } => {
                OpenGpuiConnectionIntent::reconnect(
                    candidate.edge(),
                    candidate.endpoint(),
                    candidate.opposite_port(),
                    mode,
                )
            }
            PortConnectionDragIntent::BlockedOccupied { .. } => return None,
        };
        let generation = self.connections.begin_drag(intent).ok()?;
        self.last_create_from_wire = None;
        self.connection_owner = Some(MountedConnectionOwner {
            owner: owner.clone(),
            mount: mount.clone(),
            search_lease: None,
        });
        self.connection_preview = Some(OpenGpuiConnectionPreview {
            owner: owner.clone(),
            mount: mount.clone(),
            generation,
            intent,
            pointer,
        });
        Some(generation)
    }

    pub(super) fn update_connection_preview(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        position: open_gpui::Point<Pixels>,
    ) {
        let Some(pointer) = self.canvas_position(position) else {
            return;
        };
        let generation = self.connections.generation();
        let phase = self.connections.phase();
        let Some(preview) = self.connection_preview.as_mut() else {
            return;
        };
        if generation != Some(preview.generation)
            || phase != OpenGpuiConnectionPhase::Dragging
            || preview.owner != *owner
            || !preview.mount.is_same_mount(mount)
            || !mount.is_live()
        {
            return;
        }
        preview.pointer = pointer;
    }

    fn finish_connection_drag(
        &mut self,
        position: open_gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.connections.generation() else {
            return;
        };
        let Some(canvas_position) = self.canvas_position(position) else {
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
            return;
        };
        if let Some(target) = self.hit_test_port(canvas_position) {
            self.commit_connection(generation, target, Some(cx));
            return;
        }
        self.request_create_from_wire_search(generation, canvas_position);
    }

    fn request_create_from_wire_search(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        position: CanvasPoint,
    ) {
        let request = match self.connections.request_create_from_wire(
            generation,
            OpenGpuiConnectionPoint::new(position.x, position.y),
        ) {
            Ok(request) => request,
            Err(_) => {
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                return;
            }
        };
        let Some((instance, mount)) = self
            .connection_owner
            .as_ref()
            .map(|owner| (owner.owner.instance().clone(), owner.mount.clone()))
        else {
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
            return;
        };
        let activity = self.mounts.get(&instance).and_then(|mounted| {
            mounted
                .mount
                .is_same_mount(&mount)
                .then(|| mounted.activity.clone())
        });
        let Some(activity) = activity else {
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::SourceRetired);
            return;
        };
        let Ok(lease) = activity.acquire(OpenGpuiActivityKind::Overlay) else {
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
            return;
        };
        let Some(owner) = self.connection_owner.as_mut() else {
            drop(lease);
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
            return;
        };
        owner.retain_search(lease);
        self.last_create_from_wire = Some(request);
    }

    pub(super) fn commit_connection(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
        cx: Option<&mut Context<Self>>,
    ) {
        let weak_view = self.view.downgrade();
        let fresh_revision = || {
            weak_view
                .graph_snapshot()
                .ok()
                .map(|snapshot| snapshot.revision())
        };
        let weak_view = self.view.downgrade();
        let fresh_geometry = || {
            weak_view
                .geometry_snapshot()
                .ok()
                .is_some_and(|geometry| geometry.port(target).is_some())
        };
        let prepared =
            self.connections
                .commit_to_port(generation, target, fresh_revision, fresh_geometry);
        let commit = match prepared {
            Ok(commit) => commit,
            Err(rejection) => {
                self.record_connection_commit(
                    generation,
                    target,
                    OpenGpuiConnectionCommitOutcome::Failed(
                        OpenGpuiConnectionCommitFailure::Preparation(rejection),
                    ),
                );
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                return;
            }
        };
        match self.view.evaluate_connection(commit.edit()) {
            Ok(evaluation) if evaluation.decision() == ConnectionDecision::Accept => {}
            Ok(evaluation) => {
                self.record_connection_commit(
                    generation,
                    target,
                    OpenGpuiConnectionCommitOutcome::Failed(
                        OpenGpuiConnectionCommitFailure::PolicyRejected(evaluation),
                    ),
                );
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                return;
            }
            Err(error) => {
                self.record_connection_commit(
                    generation,
                    target,
                    OpenGpuiConnectionCommitOutcome::Failed(
                        OpenGpuiConnectionCommitFailure::Session {
                            stage: OpenGpuiConnectionCommitStage::Evaluation,
                            error,
                        },
                    ),
                );
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                return;
            }
        }
        if let Err(error) = self.commands.enqueue_detached(
            commit.into_command(),
            OpenGpuiCommandOrigin::Connection { generation, target },
        ) {
            self.record_connection_commit(
                generation,
                target,
                OpenGpuiConnectionCommitOutcome::Failed(
                    OpenGpuiConnectionCommitFailure::CommandEnqueue(error),
                ),
            );
            let _ = self.connections.reject_commit(generation);
            self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
        } else if let Some(cx) = cx {
            cx.notify();
        }
    }

    pub(super) fn handle_create_from_wire_result(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        result: &Result<GraphCommandOutcome, OpenGpuiCommandTicketError>,
    ) {
        match result {
            Ok(GraphCommandOutcome::Applied(_) | GraphCommandOutcome::Noop(_)) => {
                if self.connections.finish_commit(generation)
                    == OpenGpuiConnectionTransition::Applied
                {
                    self.clear_connection_session();
                }
            }
            Ok(GraphCommandOutcome::Conflict(_) | GraphCommandOutcome::Rejected(_)) | Err(_) => {
                let _ = self.connections.reject_create_from_wire_commit(generation);
            }
        }
    }

    pub(super) fn handle_connection_result(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
        result: Result<GraphCommandOutcome, OpenGpuiCommandTicketError>,
    ) {
        let outcome = match result {
            Ok(GraphCommandOutcome::Applied(receipt)) => {
                if self.connections.finish_commit(generation)
                    == OpenGpuiConnectionTransition::Applied
                {
                    self.clear_connection_session();
                }
                OpenGpuiConnectionCommitOutcome::Applied(receipt)
            }
            Ok(GraphCommandOutcome::Noop(noop)) => {
                if self.connections.finish_commit(generation)
                    == OpenGpuiConnectionTransition::Applied
                {
                    self.clear_connection_session();
                }
                OpenGpuiConnectionCommitOutcome::Noop(noop)
            }
            Ok(GraphCommandOutcome::Conflict(conflict)) => {
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                OpenGpuiConnectionCommitOutcome::Failed(OpenGpuiConnectionCommitFailure::Conflict(
                    conflict,
                ))
            }
            Ok(GraphCommandOutcome::Rejected(rejection)) => {
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                OpenGpuiConnectionCommitOutcome::Failed(OpenGpuiConnectionCommitFailure::Rejected(
                    rejection,
                ))
            }
            Err(OpenGpuiCommandTicketError::Session(error)) => {
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                OpenGpuiConnectionCommitOutcome::Failed(OpenGpuiConnectionCommitFailure::Session {
                    stage: OpenGpuiConnectionCommitStage::Execution,
                    error,
                })
            }
            Err(error) => {
                let _ = self.connections.reject_commit(generation);
                self.abort_connection(generation, OpenGpuiConnectionCancelReason::CommandRejected);
                OpenGpuiConnectionCommitOutcome::Failed(OpenGpuiConnectionCommitFailure::Command(
                    error,
                ))
            }
        };
        self.record_connection_commit(generation, target, outcome);
    }

    fn record_connection_commit(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
        outcome: OpenGpuiConnectionCommitOutcome,
    ) {
        self.last_connection_commit = Some(OpenGpuiConnectionCommitEvent {
            generation,
            target,
            outcome,
        });
    }

    pub(super) fn cancel_connection_for_drag(&mut self, reason: OpenGpuiPortDragCancelReason) {
        let reason = match reason {
            OpenGpuiPortDragCancelReason::Escape => OpenGpuiConnectionCancelReason::Escape,
            OpenGpuiPortDragCancelReason::OwnerRetired => {
                OpenGpuiConnectionCancelReason::SourceRetired
            }
            OpenGpuiPortDragCancelReason::HostClosed => OpenGpuiConnectionCancelReason::HostClosed,
            OpenGpuiPortDragCancelReason::PortDisabled
            | OpenGpuiPortDragCancelReason::ProtectedCapacityExceeded
            | OpenGpuiPortDragCancelReason::InteractionUnavailable => {
                OpenGpuiConnectionCancelReason::CommandRejected
            }
        };
        if let Some(generation) = self.connections.generation() {
            self.abort_connection(generation, reason);
        }
    }

    pub(super) fn abort_connection(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        reason: OpenGpuiConnectionCancelReason,
    ) {
        if self.connections.abort(generation, reason) != OpenGpuiConnectionTransition::Applied {
            return;
        }
        self.clear_connection_session();
    }

    fn clear_connection_session(&mut self) {
        self.connection_owner = None;
        self.connection_preview = None;
        self.last_create_from_wire = None;
    }

    fn connection_mode(&self) -> NodeGraphConnectionMode {
        NodeGraphConnectionMode::default()
    }

    pub(super) fn canvas_position(
        &self,
        position: open_gpui::Point<Pixels>,
    ) -> Option<CanvasPoint> {
        let transform = self.scene_transform?;
        self.scene_screen_position(position)
            .map(|screen| transform.canvas_point_at_screen(screen))
    }

    fn hit_test_port(&self, position: CanvasPoint) -> Option<PortId> {
        let excluded = self.connections.target_port();
        let snapshot = self.view.graph_snapshot().ok()?;
        let geometry = self.view.geometry_snapshot().ok()?;
        let mut matching_by_node = BTreeMap::<NodeId, (PortId, f32)>::new();
        for (port, port_geometry) in geometry.ports() {
            if Some(*port) == excluded || !rect_contains(port_geometry.hit_bounds(), position) {
                continue;
            }
            let Some(node) = snapshot.graph().port(port).map(|port| port.node) else {
                continue;
            };
            let anchor = port_geometry.route_anchor();
            let dx = anchor.x - position.x;
            let dy = anchor.y - position.y;
            let distance_squared = dx.mul_add(dx, dy * dy);
            let replace = matching_by_node
                .get(&node)
                .is_none_or(|(current, distance)| {
                    distance_squared
                        .total_cmp(distance)
                        .then_with(|| port.cmp(current))
                        .is_lt()
                });
            if replace {
                matching_by_node.insert(node, (*port, distance_squared));
            }
        }

        let zoom = self.scene_transform?.zoom;
        let mut topmost = None;
        for pass in [
            OpenGpuiNodePresentation::Overview,
            OpenGpuiNodePresentation::Native,
        ] {
            for (instance, mounted) in self.mounts.active_in_order() {
                let presentation =
                    OpenGpuiNodePresentation::for_zoom(zoom, mounted.activity.protected_count());
                if presentation == pass
                    && let Some((port, _)) = matching_by_node.get(&instance.node())
                {
                    topmost = Some(*port);
                }
            }
        }
        topmost
    }

    pub(super) fn retire_connection_owner(
        &mut self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) {
        let owns_connection = self.connection_owner.as_ref().is_some_and(|current| {
            current.owner.instance() == instance && current.mount.is_same_mount(mount)
        });
        if owns_connection {
            self.cancel_connection_for_drag(OpenGpuiPortDragCancelReason::OwnerRetired);
        }
    }

    pub(super) fn connection_preview_plan(
        &self,
        geometry: &GeometrySnapshot,
    ) -> Option<OpenGpuiConnectionPreviewPlan> {
        let preview = self.connection_preview.as_ref()?;
        if self.connections.generation() != Some(preview.generation)
            || !matches!(
                self.connections.phase(),
                OpenGpuiConnectionPhase::Dragging | OpenGpuiConnectionPhase::Searching
            )
            || !preview.mount.is_live()
            || !self
                .connection_owner
                .as_ref()
                .is_some_and(|owner| owner.matches(&preview.owner, &preview.mount))
            || !self
                .mount_capability(preview.owner.instance())
                .is_some_and(|mount| mount.is_same_mount(&preview.mount))
        {
            return None;
        }

        let snapshot = self.view.graph_snapshot().ok()?;
        let graph = snapshot.graph();
        let owner_port = graph.port(&preview.owner.port())?;
        if owner_port.node != preview.owner.instance().node()
            || !matches!(
                self.view.node_incarnation(owner_port.node),
                Ok(Some(incarnation)) if incarnation == preview.owner.instance().incarnation()
            )
        {
            return None;
        }

        let endpoint = |port: PortId| {
            let geometry = geometry.port(port)?;
            let point = geometry.route_anchor();
            point.is_finite().then_some(EdgeEndpointPosition {
                point,
                position: geometry.outward(),
            })
        };
        let free_endpoint = |position: HandlePosition| EdgeEndpointPosition {
            point: preview.pointer,
            position: opposite_handle_position(position),
        };

        match preview.intent {
            OpenGpuiConnectionIntent::Connect { source, .. } => {
                let anchored = endpoint(source)?;
                let free = free_endpoint(anchored.position);
                let endpoints = match graph.port(&source)?.dir {
                    PortDirection::Out => EdgePosition {
                        source: anchored,
                        target: free,
                    },
                    PortDirection::In => EdgePosition {
                        source: free,
                        target: anchored,
                    },
                };
                let path = bezier_edge_path(
                    endpoints.source,
                    endpoints.target,
                    BezierEdgeOptions::default(),
                )?;
                Some(OpenGpuiConnectionPreviewPlan {
                    path,
                    suppressed_edge: None,
                })
            }
            OpenGpuiConnectionIntent::Reconnect {
                edge,
                endpoint: moving_endpoint,
                fixed_port,
                ..
            } => {
                let fixed = endpoint(fixed_port)?;
                let moving_position = geometry.port(preview.owner.port()).map_or_else(
                    || opposite_handle_position(fixed.position),
                    |port| port.outward(),
                );
                let moving = EdgeEndpointPosition {
                    point: preview.pointer,
                    position: moving_position,
                };
                let endpoints = match moving_endpoint {
                    jellyflow::runtime::EdgeEndpoint::From => EdgePosition {
                        source: moving,
                        target: fixed,
                    },
                    jellyflow::runtime::EdgeEndpoint::To => EdgePosition {
                        source: fixed,
                        target: moving,
                    },
                };
                let path = resolve_edge_route_path(graph.edge(&edge)?, endpoints)?.path;
                Some(OpenGpuiConnectionPreviewPlan {
                    path,
                    suppressed_edge: Some(edge),
                })
            }
        }
    }
}

fn rect_contains(bounds: CanvasRect, point: CanvasPoint) -> bool {
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y <= bounds.origin.y + bounds.size.height
}

fn opposite_handle_position(position: HandlePosition) -> HandlePosition {
    match position {
        HandlePosition::Top => HandlePosition::Bottom,
        HandlePosition::Right => HandlePosition::Left,
        HandlePosition::Bottom => HandlePosition::Top,
        HandlePosition::Left => HandlePosition::Right,
    }
}
