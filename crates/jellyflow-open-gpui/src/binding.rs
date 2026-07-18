use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::{Node, NodeId, Port, PortDirection, PortId};
use jellyflow::runtime::GraphSnapshot;
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{
    FrameNode, GraphCommand, GraphRevision, NodeInstanceKey, PortConnectionFacts, ViewSessionError,
    WeakViewSession,
};
use open_gpui::{AnyElement, App, Context, FocusHandle, IntoElement};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::command_journal::{
    OpenGpuiCommandEnqueueError, OpenGpuiCommandSink, OpenGpuiCommandTicket,
};
use crate::node_drag_handle::OpenGpuiNodeDragHandle;
use crate::port_handle::{OpenGpuiMeasurementSink, OpenGpuiPortHandle};
use crate::presentation::{OpenGpuiNodePresentationSnapshot, OpenGpuiPortInteractionState};

/// Revocable identity for one adapter-owned native mount.
///
/// Runtime node incarnations deliberately survive cache eviction and explicit remounts. This
/// capability distinguishes those fresh native mounts so callbacks retained by an older entity
/// cannot regain authority when the same [`NodeInstanceKey`] appears again.
#[derive(Clone)]
pub(crate) struct OpenGpuiMountCapability {
    live: Rc<Cell<bool>>,
}

impl OpenGpuiMountCapability {
    pub(crate) fn new() -> Self {
        Self {
            live: Rc::new(Cell::new(true)),
        }
    }

    pub(crate) fn revoke(&self) {
        self.live.set(false);
    }

    pub(crate) fn is_live(&self) -> bool {
        self.live.get()
    }

    pub(crate) fn is_same_mount(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.live, &other.live)
    }
}

impl std::fmt::Debug for OpenGpuiMountCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiMountCapability")
            .field("live", &self.is_live())
            .finish()
    }
}

pub(crate) type OpenGpuiLayoutInvalidator = Rc<
    dyn Fn(&NodeInstanceKey, &OpenGpuiMountCapability, &mut App) -> Result<bool, ViewSessionError>
        + 'static,
>;

/// One real graph port exposed to a retained native node.
#[derive(Debug, Clone, Copy)]
pub struct OpenGpuiBoundPort<'a> {
    id: PortId,
    port: &'a Port,
}

impl OpenGpuiBoundPort<'_> {
    pub fn id(&self) -> PortId {
        self.id
    }

    pub fn port(&self) -> &Port {
        self.port
    }
}

/// Immutable graph facts used to reconcile one retained native node.
///
/// The snapshot deliberately excludes frame tokens and layout targets. Those values expire every
/// frame and belong to the host-scoped measurement collector rather than retained component state.
#[derive(Clone)]
pub struct OpenGpuiNodeBindingSnapshot {
    instance: NodeInstanceKey,
    facts_revision: GraphRevision,
    node_drag_enabled: bool,
    port_facts: Vec<(PortId, PortDirection, String)>,
    view: WeakViewSession,
    mount: OpenGpuiMountCapability,
    commands: OpenGpuiCommandSink,
}

impl std::fmt::Debug for OpenGpuiNodeBindingSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiNodeBindingSnapshot")
            .field("instance", &self.instance)
            .field("facts_revision", &self.facts_revision)
            .field("node_drag_enabled", &self.node_drag_enabled)
            .field("port_count", &self.port_facts.len())
            .finish_non_exhaustive()
    }
}

/// Short-lived immutable read of the latest graph facts for one retained node.
///
/// Dropping this value releases its graph-version root. Retained entities should keep the binding
/// snapshot and acquire a read only while rendering or handling an input event.
pub struct OpenGpuiNodeBindingRead {
    instance: NodeInstanceKey,
    snapshot: GraphSnapshot,
    view: WeakViewSession,
    mount: OpenGpuiMountCapability,
    commands: OpenGpuiCommandSink,
}

impl std::fmt::Debug for OpenGpuiNodeBindingRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiNodeBindingRead")
            .field("instance", &self.instance)
            .field("graph_revision", &self.snapshot.revision())
            .finish_non_exhaustive()
    }
}

impl OpenGpuiNodeBindingSnapshot {
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    pub fn node_id(&self) -> NodeId {
        self.instance.node()
    }

    pub fn renderer_key(&self) -> &str {
        self.instance.renderer().key()
    }

    /// Latest graph revision that changed facts observable by this native node.
    ///
    /// This is deliberately not the current global graph revision and must not be reused as a
    /// command's expected revision.
    pub fn facts_revision(&self) -> GraphRevision {
        self.facts_revision
    }

    pub fn read(&self) -> Result<OpenGpuiNodeBindingRead, OpenGpuiNodeBindingReadError> {
        self.ensure_mounted()?;
        let current_incarnation = self.view.node_incarnation(self.node_id())?;
        if current_incarnation != Some(self.instance.incarnation()) {
            return Err(OpenGpuiNodeBindingReadError::Retired {
                node: self.node_id(),
            });
        }
        let snapshot = self.view.graph_snapshot()?;
        validate_snapshot_node(&snapshot, self.node_id())?;
        Ok(OpenGpuiNodeBindingRead {
            instance: self.instance.clone(),
            snapshot,
            view: self.view.clone(),
            mount: self.mount.clone(),
            commands: self.commands.clone(),
        })
    }

    pub(crate) fn from_frame(
        frame: &FrameNode,
        view: WeakViewSession,
        mount: OpenGpuiMountCapability,
        commands: OpenGpuiCommandSink,
    ) -> Result<Self, OpenGpuiNodeBindingError> {
        validate_frame_node(frame)?;
        Ok(Self {
            instance: frame.instance().clone(),
            facts_revision: frame.binding_revision(),
            node_drag_enabled: frame.node().is_some_and(|node| {
                node.selectable.unwrap_or(true) && node.draggable.unwrap_or(true)
            }),
            port_facts: frame
                .ports()
                .iter()
                .filter_map(|frame_port| {
                    frame_port
                        .port()
                        .map(|port| (frame_port.id(), port.dir, port.key.0.clone()))
                })
                .collect(),
            view,
            mount,
            commands,
        })
    }

    pub(crate) fn advance(&mut self, facts_revision: GraphRevision) {
        if let Ok(read) = self.read() {
            self.node_drag_enabled =
                read.node().selectable.unwrap_or(true) && read.node().draggable.unwrap_or(true);
            self.port_facts = read
                .ports()
                .map(|port| (port.id(), port.port().dir, port.port().key.0.clone()))
                .collect();
        }
        self.facts_revision = self.facts_revision.max(facts_revision);
    }

    fn cached_port_facts(&self, port: PortId) -> Option<(PortDirection, &str)> {
        self.port_facts
            .iter()
            .find_map(|(id, direction, name)| (*id == port).then_some((*direction, name.as_str())))
    }

    fn ensure_mounted(&self) -> Result<(), OpenGpuiNodeBindingReadError> {
        if self.mount.is_live() {
            Ok(())
        } else {
            Err(OpenGpuiNodeBindingReadError::Retired {
                node: self.node_id(),
            })
        }
    }
}

impl OpenGpuiNodeBindingRead {
    /// Current global graph revision to use when planning a revision-checked command.
    pub fn graph_revision(&self) -> GraphRevision {
        self.snapshot.revision()
    }

    /// Returns the current view transform for a component-managed scale policy.
    ///
    /// The host does not synthesize arbitrary subtree transforms. A renderer that opts into
    /// component-managed scaling uses this value to choose coherent native metrics itself.
    pub fn viewport_transform(&self) -> Result<ViewportTransform, OpenGpuiNodeBindingReadError> {
        self.ensure_current_instance()?;
        self.view.viewport_transform().map_err(Into::into)
    }

    pub fn node(&self) -> &Node {
        self.snapshot
            .graph()
            .node(&self.instance.node())
            .expect("a binding read keeps its validated immutable node")
    }

    pub fn ports(&self) -> impl Iterator<Item = OpenGpuiBoundPort<'_>> + '_ {
        self.node().ports.iter().filter_map(|id| {
            self.snapshot
                .graph()
                .port(id)
                .map(|port| OpenGpuiBoundPort { id: *id, port })
        })
    }

    pub fn port(&self, id: PortId) -> Option<&Port> {
        self.node()
            .ports
            .contains(&id)
            .then(|| self.snapshot.graph().port(&id))
            .flatten()
    }

    /// Returns revision-bound connection facts for one real port owned by this node.
    ///
    /// The view session resolves connectability policy and incident edges. Native controls can use
    /// these facts to switch between an inline value editor and a connected-input presentation
    /// without scanning graph edges or reimplementing runtime policy.
    pub fn port_connection_facts(
        &self,
        id: PortId,
    ) -> Result<Option<PortConnectionFacts>, OpenGpuiNodeBindingReadError> {
        self.ensure_current_instance()?;
        if self.port(id).is_none() {
            return Ok(None);
        }
        self.view.port_connection_facts(id).map_err(Into::into)
    }

    /// Builds and enqueues one command against this read's authoritative graph revision.
    ///
    /// The builder may reconcile local state and inspect graph facts while this immutable root is
    /// alive. The command must use [`Self::graph_revision`] as its expected revision. Dispatch
    /// consumes the read so callers cannot continue using the old root after enqueueing. The
    /// returned ticket resolves only after the host submits a real frame feedback batch.
    pub fn enqueue_command(
        self,
        cx: &mut App,
        build_command: impl FnOnce(&Self) -> GraphCommand,
    ) -> Result<OpenGpuiCommandTicket, OpenGpuiNodeBindingReadError> {
        self.ensure_current_instance()?;
        let command = build_command(&self);
        self.ensure_current_instance()?;

        let read_revision = self.graph_revision();
        let command_revision = command.expected_revision();
        if command_revision != read_revision {
            return Err(OpenGpuiNodeBindingReadError::CommandRevisionMismatch {
                read_revision,
                command_revision,
            });
        }

        self.commands.enqueue(command, cx).map_err(Into::into)
    }

    fn ensure_current_instance(&self) -> Result<(), OpenGpuiNodeBindingReadError> {
        let node = self.instance.node();
        if !self.mount.is_live() {
            return Err(OpenGpuiNodeBindingReadError::Retired { node });
        }
        let current_incarnation = self.view.node_incarnation(node)?;
        if current_incarnation != Some(self.instance.incarnation()) {
            return Err(OpenGpuiNodeBindingReadError::Retired { node });
        }
        Ok(())
    }
}

/// Retained, adapter-owned semantic binding passed to native Open GPUI node entities.
pub struct OpenGpuiNodeBinding {
    snapshot: OpenGpuiNodeBindingSnapshot,
    presentation: OpenGpuiNodePresentationSnapshot,
    port_focus: BTreeMap<PortId, FocusHandle>,
    activity: OpenGpuiNodeActivityRegistry,
    drag_handle: OpenGpuiNodeDragHandle,
    measurement: OpenGpuiMeasurementSink,
    layout_invalidator: OpenGpuiLayoutInvalidator,
}

impl std::fmt::Debug for OpenGpuiNodeBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiNodeBinding")
            .field("snapshot", &self.snapshot)
            .field("presentation", &self.presentation)
            .field("activity", &self.activity)
            .field("drag_handle", &self.drag_handle)
            .field("measurement", &self.measurement)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiNodeBinding {
    pub fn snapshot(&self) -> &OpenGpuiNodeBindingSnapshot {
        &self.snapshot
    }

    /// Returns the latest immutable adapter-local node and port interaction state.
    pub fn presentation(&self) -> &OpenGpuiNodePresentationSnapshot {
        &self.presentation
    }

    pub fn activity(&self) -> &OpenGpuiNodeActivityRegistry {
        &self.activity
    }

    /// Returns the explicit native pointer surface capability for this retained node instance.
    #[must_use]
    pub fn drag_handle(&self) -> OpenGpuiNodeDragHandle {
        self.drag_handle.clone()
    }

    /// Measures one native content root for a content-authoritative registration policy.
    ///
    /// The wrapped element is measured in native logical pixels before graph feedback is
    /// submitted. The host never infers content size from its zoomed graph wrapper. Renderers
    /// using document-authoritative size may omit this wrapper.
    pub fn measure_content(&self, child: impl IntoElement) -> AnyElement {
        self.measurement.measure_content(child)
    }

    /// Invalidates native geometry for this exact retained node instance.
    ///
    /// The request is routed through the owning scene host so a retired binding cannot invalidate a
    /// replacement node that happens to reuse the same [`NodeId`]. A successful invalidation asks
    /// the host to materialize and measure a fresh layout generation.
    pub fn invalidate_layout(&self, cx: &mut Context<Self>) -> Result<bool, ViewSessionError> {
        if !self.snapshot.mount.is_live() {
            return Ok(false);
        }
        (self.layout_invalidator)(&self.snapshot.instance, &self.snapshot.mount, cx)
    }

    /// Returns a semantic measurement handle for a real port owned by this node.
    #[must_use]
    pub fn port_handle(&self, port: PortId) -> Option<OpenGpuiPortHandle> {
        let (direction, name) = match self.snapshot.read() {
            Ok(read) => read.port(port).map(|port| (port.dir, port.key.0.clone())),
            Err(OpenGpuiNodeBindingReadError::Session(ViewSessionError::Closed { .. })) => self
                .snapshot
                .cached_port_facts(port)
                .map(|(direction, name)| (direction, name.to_owned())),
            Err(_) => None,
        }?;
        Some(
            OpenGpuiPortHandle::new(
                self.snapshot.instance.clone(),
                port,
                direction,
                name,
                self.measurement.clone(),
            )
            .with_focus_handle(self.port_focus.get(&port).cloned())
            .with_presentation_state(
                self.presentation
                    .port(port)
                    .cloned()
                    .unwrap_or_else(OpenGpuiPortInteractionState::idle),
            ),
        )
    }

    pub(crate) fn cached_port_handle(&self, port: PortId) -> Option<OpenGpuiPortHandle> {
        let (direction, name) = self.snapshot.cached_port_facts(port)?;
        Some(
            OpenGpuiPortHandle::new(
                self.snapshot.instance.clone(),
                port,
                direction,
                name.to_owned(),
                self.measurement.clone(),
            )
            .with_focus_handle(self.port_focus.get(&port).cloned())
            .with_presentation_state(
                self.presentation
                    .port(port)
                    .cloned()
                    .unwrap_or_else(OpenGpuiPortInteractionState::idle),
            ),
        )
    }

    /// Builds semantic measurement handles for every real port in one validated binding read.
    ///
    /// Native nodes that render multiple ports should prefer this method over repeatedly calling
    /// [`Self::port_handle`]. The caller supplies the read it already uses for node facts, so one
    /// graph snapshot acquisition and one ordered port traversal serve the entire render pass.
    pub fn port_handles(
        &self,
        read: &OpenGpuiNodeBindingRead,
    ) -> Result<Vec<OpenGpuiPortHandle>, OpenGpuiNodeBindingReadError> {
        if read.instance != self.snapshot.instance
            || !read.mount.is_same_mount(&self.snapshot.mount)
        {
            return Err(OpenGpuiNodeBindingReadError::ForeignRead);
        }
        read.ensure_current_instance()?;

        Ok(read
            .ports()
            .map(|port| {
                OpenGpuiPortHandle::new(
                    self.snapshot.instance.clone(),
                    port.id(),
                    port.port().dir,
                    port.port().key.0.clone(),
                    self.measurement.clone(),
                )
                .with_focus_handle(self.port_focus.get(&port.id()).cloned())
                .with_presentation_state(
                    self.presentation
                        .port(port.id())
                        .cloned()
                        .unwrap_or_else(OpenGpuiPortInteractionState::idle),
                )
            })
            .collect())
    }

    pub(crate) fn new(
        snapshot: OpenGpuiNodeBindingSnapshot,
        activity: OpenGpuiNodeActivityRegistry,
        drag_handle: OpenGpuiNodeDragHandle,
        measurement: OpenGpuiMeasurementSink,
        layout_invalidator: OpenGpuiLayoutInvalidator,
        cx: &mut Context<Self>,
    ) -> Self {
        let port_focus = snapshot
            .port_facts
            .iter()
            .map(|(port, _, _)| (*port, cx.focus_handle()))
            .collect();
        let presentation = OpenGpuiNodePresentationSnapshot::default();
        drag_handle.set_graph_enabled(snapshot.node_drag_enabled);
        drag_handle.set_presentation_enabled(
            !presentation.node().disabled()
                && presentation.node().lod().publishes_native_graph_geometry(),
        );
        drag_handle.set_dragging(presentation.node().dragging());
        Self {
            snapshot,
            presentation,
            port_focus,
            activity,
            drag_handle,
            measurement,
            layout_invalidator,
        }
    }

    pub(crate) fn advance(&mut self, facts_revision: GraphRevision, cx: &mut Context<Self>) {
        self.snapshot.advance(facts_revision);
        self.drag_handle
            .set_graph_enabled(self.snapshot.node_drag_enabled);
        self.port_focus
            .retain(|port, _| self.snapshot.cached_port_facts(*port).is_some());
        for (port, _, _) in &self.snapshot.port_facts {
            self.port_focus
                .entry(*port)
                .or_insert_with(|| cx.focus_handle());
        }
    }

    pub(crate) fn port_focus_handle(&self, port: PortId) -> Option<&FocusHandle> {
        self.port_focus.get(&port)
    }

    pub(crate) fn set_presentation(
        &mut self,
        presentation: OpenGpuiNodePresentationSnapshot,
    ) -> bool {
        self.drag_handle.set_presentation_enabled(
            !presentation.node().disabled()
                && presentation.node().lod().publishes_native_graph_geometry(),
        );
        self.drag_handle
            .set_dragging(presentation.node().dragging());
        if self.presentation == presentation {
            return false;
        }
        self.presentation = presentation;
        true
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiNodeBindingReadError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("native node binding has been retired: {node:?}")]
    Retired { node: NodeId },
    #[error("node is missing from the current graph snapshot: {node:?}")]
    MissingNode { node: NodeId },
    #[error("port {port:?} is missing from node {node:?}'s current graph snapshot")]
    MissingPort { node: NodeId, port: PortId },
    #[error("binding read belongs to a different retained node instance")]
    ForeignRead,
    #[error(
        "binding command revision {command_revision} does not match read revision {read_revision}"
    )]
    CommandRevisionMismatch {
        read_revision: GraphRevision,
        command_revision: GraphRevision,
    },
    #[error(transparent)]
    CommandEnqueue(#[from] OpenGpuiCommandEnqueueError),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub(crate) enum OpenGpuiNodeBindingError {
    #[error("frame node is missing from its graph snapshot: {node:?}")]
    MissingNode { node: NodeId },
    #[error("frame port {port:?} is missing from node {node:?}'s graph snapshot")]
    MissingPort { node: NodeId, port: PortId },
}

fn validate_frame_node(frame: &FrameNode) -> Result<(), OpenGpuiNodeBindingError> {
    let node = frame
        .node()
        .ok_or(OpenGpuiNodeBindingError::MissingNode { node: frame.id() })?;
    for id in &node.ports {
        if frame.graph_snapshot().graph().port(id).is_none() {
            return Err(OpenGpuiNodeBindingError::MissingPort {
                node: frame.id(),
                port: *id,
            });
        }
    }
    Ok(())
}

fn validate_snapshot_node(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
) -> Result<(), OpenGpuiNodeBindingReadError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(OpenGpuiNodeBindingReadError::MissingNode { node: node_id })?;
    for id in &node.ports {
        if snapshot.graph().port(id).is_none() {
            return Err(OpenGpuiNodeBindingReadError::MissingPort {
                node: node_id,
                port: *id,
            });
        }
    }
    Ok(())
}
