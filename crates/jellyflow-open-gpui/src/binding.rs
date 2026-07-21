use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow_core::{NodeId, PortDirection, PortId};
use jellyflow_runtime::{
    FrameNode, GraphRevision, NodeInstanceKey, ViewCloseReason, ViewSessionError, WeakViewSession,
};
use open_gpui::{AnyElement, App, Context, FocusHandle, IntoElement};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError};
use crate::command_journal::OpenGpuiCommandSink;
use crate::interaction::OpenGpuiOverlayInteractionBoundary;
use crate::node_drag_handle::OpenGpuiNodeDragHandle;
use crate::port_handle::{OpenGpuiMeasurementSink, OpenGpuiPortHandle};
use crate::presentation::{OpenGpuiNodePresentationSnapshot, OpenGpuiPortInteractionState};
use crate::retained_field::RetainedFieldMountCapability;

/// Revocable identity for one adapter-owned native mount.
///
/// Runtime node incarnations deliberately survive cache eviction and explicit remounts. This
/// capability distinguishes those fresh native mounts so callbacks retained by an older entity
/// cannot regain authority when the same [`NodeInstanceKey`] appears again.
#[derive(Clone)]
pub(crate) struct OpenGpuiMountCapability {
    state: Rc<Cell<OpenGpuiMountState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenGpuiMountState {
    Live,
    Retired,
    Closed(ViewCloseReason),
}

impl OpenGpuiMountCapability {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(Cell::new(OpenGpuiMountState::Live)),
        }
    }

    pub(crate) fn retire(&self) {
        if self.state.get() == OpenGpuiMountState::Live {
            self.state.set(OpenGpuiMountState::Retired);
        }
    }

    pub(crate) fn close(&self, reason: ViewCloseReason) {
        if !matches!(self.state.get(), OpenGpuiMountState::Closed(_)) {
            self.state.set(OpenGpuiMountState::Closed(reason));
        }
    }

    pub(crate) fn state(&self) -> OpenGpuiMountState {
        self.state.get()
    }

    pub(crate) fn is_live(&self) -> bool {
        self.state() == OpenGpuiMountState::Live
    }

    pub(crate) fn is_same_mount(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

impl std::fmt::Debug for OpenGpuiMountCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiMountCapability")
            .field("state", &self.state())
            .finish()
    }
}

pub(crate) type OpenGpuiLayoutInvalidator = Rc<
    dyn Fn(&NodeInstanceKey, &OpenGpuiMountCapability, &mut App) -> Result<bool, ViewSessionError>
        + 'static,
>;

#[derive(Clone)]
pub(crate) struct OpenGpuiBindingMount {
    instance: NodeInstanceKey,
    facts_revision: GraphRevision,
    node_drag_enabled: bool,
    port_facts: Vec<(PortId, PortDirection, String)>,
    view: WeakViewSession,
    mount: OpenGpuiMountCapability,
    commands: OpenGpuiCommandSink,
}

impl std::fmt::Debug for OpenGpuiBindingMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiBindingMount")
            .field("instance", &self.instance)
            .field("facts_revision", &self.facts_revision)
            .field("node_drag_enabled", &self.node_drag_enabled)
            .field("port_count", &self.port_facts.len())
            .finish_non_exhaustive()
    }
}

impl OpenGpuiBindingMount {
    fn node_id(&self) -> NodeId {
        self.instance.node()
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
        if self.mount.is_live()
            && self.view.node_incarnation(self.node_id()).ok()
                == Some(Some(self.instance.incarnation()))
            && let Ok(snapshot) = self.view.graph_snapshot()
            && let Some(node) = snapshot.graph().node(&self.node_id())
        {
            self.node_drag_enabled =
                node.selectable.unwrap_or(true) && node.draggable.unwrap_or(true);
            self.port_facts = node
                .ports
                .iter()
                .filter_map(|port_id| {
                    snapshot
                        .graph()
                        .port(port_id)
                        .map(|port| (*port_id, port.dir, port.key.0.clone()))
                })
                .collect();
        }
        self.facts_revision = self.facts_revision.max(facts_revision);
    }

    fn cached_port_facts(&self, port: PortId) -> Option<(PortDirection, &str)> {
        self.port_facts
            .iter()
            .find_map(|(id, direction, name)| (*id == port).then_some((*direction, name.as_str())))
    }
}

/// Retained, adapter-owned semantic binding passed to native Open GPUI node entities.
pub struct OpenGpuiNodeBinding {
    mount: OpenGpuiBindingMount,
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
            .field("mount", &self.mount)
            .field("presentation", &self.presentation)
            .field("activity", &self.activity)
            .field("drag_handle", &self.drag_handle)
            .field("measurement", &self.measurement)
            .finish_non_exhaustive()
    }
}

impl OpenGpuiNodeBinding {
    /// Opens one owned authoring view for this exact mount and current graph revision.
    pub fn authoring(&self) -> Result<OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError> {
        OpenGpuiNodeAuthoring::open(
            self.mount.instance.clone(),
            self.mount.view.clone(),
            self.mount.mount.clone(),
            self.mount.commands.clone(),
            self.measurement.clone(),
            &self.port_focus,
            self.presentation.clone(),
        )
    }

    /// Returns the latest immutable adapter-local node and port interaction state.
    pub fn presentation(&self) -> &OpenGpuiNodePresentationSnapshot {
        &self.presentation
    }

    pub fn activity(&self) -> &OpenGpuiNodeActivityRegistry {
        &self.activity
    }

    pub(crate) fn retained_field_mount(&self) -> RetainedFieldMountCapability {
        RetainedFieldMountCapability::new(
            self.mount.instance.clone(),
            self.mount.view.clone(),
            self.mount.mount.clone(),
            self.activity.clone(),
        )
    }

    /// Returns the explicit native pointer surface capability for this retained node instance.
    #[must_use]
    pub fn drag_handle(&self) -> OpenGpuiNodeDragHandle {
        self.drag_handle.clone()
    }

    /// Returns the host-owned interaction boundary for a popup rendered outside node bounds.
    #[must_use]
    pub fn overlay_boundary(&self) -> OpenGpuiOverlayInteractionBoundary {
        self.drag_handle.overlay_boundary()
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
        if !self.mount.mount.is_live() {
            return Ok(false);
        }
        (self.layout_invalidator)(&self.mount.instance, &self.mount.mount, cx)
    }

    pub(crate) fn cached_port_handle(&self, port: PortId) -> Option<OpenGpuiPortHandle> {
        let (direction, name) = self.mount.cached_port_facts(port)?;
        Some(
            OpenGpuiPortHandle::new(
                self.mount.instance.clone(),
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

    pub(crate) fn new(
        mount: OpenGpuiBindingMount,
        activity: OpenGpuiNodeActivityRegistry,
        drag_handle: OpenGpuiNodeDragHandle,
        measurement: OpenGpuiMeasurementSink,
        layout_invalidator: OpenGpuiLayoutInvalidator,
        cx: &mut Context<Self>,
    ) -> Self {
        let port_focus = mount
            .port_facts
            .iter()
            .map(|(port, _, _)| (*port, cx.focus_handle()))
            .collect();
        let presentation = OpenGpuiNodePresentationSnapshot::default();
        drag_handle.set_graph_enabled(mount.node_drag_enabled);
        drag_handle.set_presentation_enabled(
            !presentation.node().disabled()
                && presentation.node().lod().publishes_native_graph_geometry(),
        );
        drag_handle.set_dragging(presentation.node().dragging());
        Self {
            mount,
            presentation,
            port_focus,
            activity,
            drag_handle,
            measurement,
            layout_invalidator,
        }
    }

    pub(crate) fn advance(&mut self, facts_revision: GraphRevision, cx: &mut Context<Self>) {
        self.mount.advance(facts_revision);
        self.drag_handle
            .set_graph_enabled(self.mount.node_drag_enabled);
        self.port_focus
            .retain(|port, _| self.mount.cached_port_facts(*port).is_some());
        for (port, _, _) in &self.mount.port_facts {
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
