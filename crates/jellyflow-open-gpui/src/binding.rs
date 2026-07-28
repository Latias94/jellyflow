use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use jellyflow_core::{NodeId, PortDirection, PortId};
use jellyflow_runtime::{
    graph::Diagnostic, graph::GraphRevision, graph::PortConnectionSummary, graph::PortInstanceKey,
    interaction::CanvasInteractionPhase, interaction::ConnectionGestureBinding,
    interaction::InteractionGeneration, interaction::InteractionTicketId, view::FrameNode,
    view::NodeInstanceKey, view::ViewCloseReason, view::ViewProvenance, view::ViewSessionError,
    view::WeakViewSession,
};
use open_gpui::{AnyElement, App, Context, FocusHandle, IntoElement};

use crate::activity::{OpenGpuiNodeActivityHandle, OpenGpuiNodeActivityRegistry};
use crate::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError};
use crate::command_journal::OpenGpuiCommandSink;
use crate::interaction::OpenGpuiOverlayInteractionBoundary;
use crate::node_drag_handle::OpenGpuiNodeDragHandle;
use crate::port_handle::{OpenGpuiMeasurementSink, OpenGpuiPortHandle};
use crate::presentation::{
    OpenGpuiNodeInteractionState, OpenGpuiPortCandidateState, OpenGpuiSettlementState,
};
use crate::presentation::{OpenGpuiNodePresentationSnapshot, OpenGpuiPortInteractionState};

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

/// Exact view, runtime, node-lifetime, and native-mount authority for one retained node.
///
/// Keeping these values together prevents adapter-local owners from pairing a node instance with
/// a weak capability from another view or runtime. The captured view generation also makes a
/// terminal weak capability permanently stale even if its numeric view identity is reused.
#[derive(Clone)]
pub(crate) struct OpenGpuiMountContext {
    instance: NodeInstanceKey,
    provenance: ViewProvenance,
    view: WeakViewSession,
    mount: OpenGpuiMountCapability,
}

impl OpenGpuiMountContext {
    pub(crate) fn new(
        instance: NodeInstanceKey,
        view: WeakViewSession,
        mount: OpenGpuiMountCapability,
    ) -> Result<Self, OpenGpuiMountValidationError> {
        let provenance = view.provenance().map_err(map_mount_session_error)?;
        if provenance.view() != instance.view()
            || provenance.runtime() != instance.lifetime().provenance()
        {
            return Err(OpenGpuiMountValidationError::ForeignView);
        }
        Ok(Self {
            instance,
            provenance,
            view,
            mount,
        })
    }

    pub(crate) fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    pub(crate) fn view(&self) -> &WeakViewSession {
        &self.view
    }

    pub(crate) fn mount(&self) -> &OpenGpuiMountCapability {
        &self.mount
    }

    pub(crate) fn provenance(&self) -> ViewProvenance {
        self.provenance
    }

    pub(crate) fn ensure_current(
        &self,
    ) -> Result<jellyflow_runtime::graph::GraphSnapshot, OpenGpuiMountValidationError> {
        self.ensure_exact_view()?;
        let snapshot = self
            .view
            .graph_snapshot()
            .map_err(map_mount_session_error)?;
        if snapshot.provenance() != self.provenance.runtime() {
            return Err(OpenGpuiMountValidationError::ForeignView);
        }
        if !snapshot.node_lifetime_is_current(self.instance.lifetime()) {
            return Err(OpenGpuiMountValidationError::RetiredIncarnation {
                node: self.instance.node(),
            });
        }
        self.ensure_exact_view()?;
        Ok(snapshot)
    }

    pub(crate) fn ensure_exact_view(&self) -> Result<(), OpenGpuiMountValidationError> {
        match self.mount.state() {
            OpenGpuiMountState::Live => {}
            OpenGpuiMountState::Retired => {
                return Err(OpenGpuiMountValidationError::RetiredMount {
                    node: self.instance.node(),
                });
            }
            OpenGpuiMountState::Closed(reason) => {
                return Err(OpenGpuiMountValidationError::Closed { reason });
            }
        }
        let current = self.view.provenance().map_err(map_mount_session_error)?;
        if current != self.provenance
            || current.view() != self.instance.view()
            || current.runtime() != self.instance.lifetime().provenance()
        {
            return Err(OpenGpuiMountValidationError::ForeignView);
        }
        Ok(())
    }

    fn matches_instance(&self, instance: &NodeInstanceKey) -> bool {
        self.instance == *instance
    }
}

impl std::fmt::Debug for OpenGpuiMountContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiMountContext")
            .field("instance", &self.instance)
            .field("provenance", &self.provenance)
            .field("mount", &self.mount)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiMountValidationError {
    #[error("native node mount is retired: {node:?}")]
    RetiredMount { node: NodeId },
    #[error("native node incarnation is retired: {node:?}")]
    RetiredIncarnation { node: NodeId },
    #[error("native node mount is bound to another runtime or view generation")]
    ForeignView,
    #[error("native node host is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error(transparent)]
    Session(ViewSessionError),
}

fn map_mount_session_error(error: ViewSessionError) -> OpenGpuiMountValidationError {
    match error {
        ViewSessionError::Closed { reason } => OpenGpuiMountValidationError::Closed { reason },
        error => OpenGpuiMountValidationError::Session(error),
    }
}

pub(crate) type OpenGpuiLayoutInvalidator = Rc<
    dyn Fn(&NodeInstanceKey, &OpenGpuiMountCapability, &mut App) -> Result<bool, ViewSessionError>
        + 'static,
>;

#[derive(Clone)]
pub(crate) struct OpenGpuiBindingMount {
    context: OpenGpuiMountContext,
    facts: OpenGpuiBindingFacts,
    commands: OpenGpuiCommandSink,
}

impl std::fmt::Debug for OpenGpuiBindingMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenGpuiBindingMount")
            .field("context", &self.context)
            .field("facts_revision", &self.facts.revision)
            .field("node_drag_enabled", &self.facts.node_drag_enabled)
            .field("port_count", &self.facts.port_facts.len())
            .finish_non_exhaustive()
    }
}

impl OpenGpuiBindingMount {
    pub(crate) fn from_frame(
        frame: &FrameNode,
        view: WeakViewSession,
        mount: OpenGpuiMountCapability,
        commands: OpenGpuiCommandSink,
    ) -> Result<Self, OpenGpuiNodeBindingError> {
        validate_frame_node(frame)?;
        let context = OpenGpuiMountContext::new(frame.instance().clone(), view, mount)?;
        let facts = OpenGpuiBindingFacts::from_frame(&context, frame)?;
        Ok(Self {
            context,
            facts,
            commands,
        })
    }

    fn advance_from_frame(
        &mut self,
        frame: &FrameNode,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        advance_binding_facts_from_frame(&mut self.facts, &self.context, frame)
    }

    fn advance_from_view(
        &mut self,
        facts_revision: GraphRevision,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        advance_binding_facts_from_view(&mut self.facts, &self.context, facts_revision, || {
            self.context.ensure_current()
        })
    }

    fn cached_port_facts(&self, port: PortId) -> Option<&OpenGpuiBindingPortFact> {
        self.facts.port_facts.get(port)
    }
}

fn advance_binding_facts_from_frame(
    current: &mut OpenGpuiBindingFacts,
    context: &OpenGpuiMountContext,
    frame: &FrameNode,
) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
    match context.ensure_exact_view() {
        Ok(()) => {}
        Err(OpenGpuiMountValidationError::Session(
            ViewSessionError::Busy | ViewSessionError::Closing,
        )) => return Ok(OpenGpuiBindingAdvance::Retry),
        Err(error) => return Err(error.into()),
    }
    let facts = OpenGpuiBindingFacts::from_frame(context, frame)?;
    *current = facts;
    Ok(OpenGpuiBindingAdvance::Applied)
}

fn advance_binding_facts_from_view(
    current: &mut OpenGpuiBindingFacts,
    context: &OpenGpuiMountContext,
    facts_revision: GraphRevision,
    load: impl FnOnce() -> Result<jellyflow_runtime::graph::GraphSnapshot, OpenGpuiMountValidationError>,
) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
    let snapshot = match load() {
        Ok(snapshot) => snapshot,
        Err(OpenGpuiMountValidationError::Session(
            ViewSessionError::Busy | ViewSessionError::Closing,
        )) => {
            return Ok(OpenGpuiBindingAdvance::Retry);
        }
        Err(error) => return Err(error.into()),
    };
    if snapshot.revision() < facts_revision {
        return Ok(OpenGpuiBindingAdvance::Retry);
    }
    let facts = OpenGpuiBindingFacts::from_snapshot(context, &snapshot, facts_revision)?;
    match context.ensure_exact_view() {
        Ok(()) => {}
        Err(OpenGpuiMountValidationError::Session(
            ViewSessionError::Busy | ViewSessionError::Closing,
        )) => return Ok(OpenGpuiBindingAdvance::Retry),
        Err(error) => return Err(error.into()),
    }
    *current = facts;
    Ok(OpenGpuiBindingAdvance::Applied)
}

#[derive(Clone)]
struct OpenGpuiBindingFacts {
    revision: GraphRevision,
    node_drag_enabled: bool,
    port_facts: OpenGpuiOrderedPortFacts,
}

impl OpenGpuiBindingFacts {
    fn from_frame(
        context: &OpenGpuiMountContext,
        frame: &FrameNode,
    ) -> Result<Self, OpenGpuiNodeBindingError> {
        if !context.matches_instance(frame.instance())
            || frame.graph_snapshot().provenance() != context.provenance().runtime()
            || !frame
                .graph_snapshot()
                .node_lifetime_is_current(context.instance().lifetime())
        {
            return Err(OpenGpuiNodeBindingError::ForeignFrameInstance { node: frame.id() });
        }
        let node = frame
            .node()
            .ok_or(OpenGpuiNodeBindingError::MissingNode { node: frame.id() })?;
        let port_facts = frame
            .ports()
            .iter()
            .map(|frame_port| {
                let port = frame_port
                    .port()
                    .ok_or(OpenGpuiNodeBindingError::MissingPort {
                        node: frame.id(),
                        port: frame_port.id(),
                    })?;
                if frame.graph_snapshot().port_instance_key(frame_port.id())
                    != Some(frame_port.instance())
                {
                    return Err(OpenGpuiNodeBindingError::IncoherentPortInstance {
                        node: frame.id(),
                        port: frame_port.id(),
                    });
                }
                Ok::<_, OpenGpuiNodeBindingError>(OpenGpuiBindingPortFact {
                    id: frame_port.id(),
                    instance: frame_port.instance(),
                    direction: port.dir,
                    name: Arc::from(port.key.0.as_str()),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !port_facts
            .iter()
            .map(|fact| fact.id)
            .eq(node.ports.iter().copied())
        {
            return Err(OpenGpuiNodeBindingError::IncoherentPortBatch { node: frame.id() });
        }
        Ok(Self {
            revision: frame.binding_revision(),
            node_drag_enabled: node.selectable.unwrap_or(true) && node.draggable.unwrap_or(true),
            port_facts: OpenGpuiOrderedPortFacts::new(port_facts),
        })
    }

    fn from_snapshot(
        context: &OpenGpuiMountContext,
        snapshot: &jellyflow_runtime::graph::GraphSnapshot,
        revision: GraphRevision,
    ) -> Result<Self, OpenGpuiNodeBindingError> {
        let node_id = context.instance().node();
        let node = snapshot
            .graph()
            .node(&node_id)
            .ok_or(OpenGpuiNodeBindingError::MissingNode { node: node_id })?;
        let port_facts = node
            .ports
            .iter()
            .map(|port_id| {
                let port = snapshot.graph().port(port_id).ok_or(
                    OpenGpuiNodeBindingError::MissingPort {
                        node: node_id,
                        port: *port_id,
                    },
                )?;
                let instance = snapshot.port_instance_key(*port_id).ok_or(
                    OpenGpuiNodeBindingError::IncoherentPortInstance {
                        node: node_id,
                        port: *port_id,
                    },
                )?;
                Ok::<_, OpenGpuiNodeBindingError>(OpenGpuiBindingPortFact {
                    id: *port_id,
                    instance,
                    direction: port.dir,
                    name: Arc::from(port.key.0.as_str()),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            revision,
            node_drag_enabled: node.selectable.unwrap_or(true) && node.draggable.unwrap_or(true),
            port_facts: OpenGpuiOrderedPortFacts::new(port_facts),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenGpuiBindingAdvance {
    Applied,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenGpuiBindingPortFact {
    id: PortId,
    instance: PortInstanceKey,
    direction: PortDirection,
    name: Arc<str>,
}

#[derive(Clone, Default)]
struct OpenGpuiOrderedPortFacts {
    ordered: Vec<OpenGpuiBindingPortFact>,
    index: BTreeMap<PortId, usize>,
}

impl std::fmt::Debug for OpenGpuiOrderedPortFacts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenGpuiOrderedPortFacts")
            .field("len", &self.ordered.len())
            .finish_non_exhaustive()
    }
}

impl OpenGpuiOrderedPortFacts {
    fn new(facts: impl IntoIterator<Item = OpenGpuiBindingPortFact>) -> Self {
        let ordered = facts.into_iter().collect::<Vec<_>>();
        let index = ordered
            .iter()
            .enumerate()
            .map(|(index, fact)| (fact.id, index))
            .collect();
        Self { ordered, index }
    }

    fn len(&self) -> usize {
        self.ordered.len()
    }

    fn get(&self, port: PortId) -> Option<&OpenGpuiBindingPortFact> {
        self.index
            .get(&port)
            .and_then(|index| self.ordered.get(*index))
    }

    fn iter(&self) -> impl ExactSizeIterator<Item = &OpenGpuiBindingPortFact> {
        self.ordered.iter()
    }

    fn ids(&self) -> impl ExactSizeIterator<Item = PortId> + '_ {
        self.ordered.iter().map(|fact| fact.id)
    }
}

struct OpenGpuiBindingPortCapability {
    instance: PortInstanceKey,
    direction: PortDirection,
    name: Arc<str>,
    focus: FocusHandle,
    handle: OpenGpuiPortHandle,
}

impl OpenGpuiBindingPortCapability {
    fn matches(&self, fact: &OpenGpuiBindingPortFact) -> bool {
        self.instance == fact.instance && self.direction == fact.direction && self.name == fact.name
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenGpuiPortCandidateInput {
    Idle,
    Source,
    ValidTarget,
    InvalidTarget(Arc<[Diagnostic]>),
}

impl OpenGpuiPortCandidateInput {
    pub(crate) fn invalid(diagnostics: &[Diagnostic]) -> Self {
        Self::InvalidTarget(Arc::from(diagnostics))
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn materialize(&self) -> OpenGpuiPortCandidateState {
        match self {
            Self::Idle => OpenGpuiPortCandidateState::Idle,
            Self::Source => OpenGpuiPortCandidateState::Source,
            Self::ValidTarget => OpenGpuiPortCandidateState::ValidTarget,
            Self::InvalidTarget(diagnostics) => {
                OpenGpuiPortCandidateState::invalid(Arc::clone(diagnostics))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenGpuiConnectionCandidateInput {
    port: PortId,
    probe_revision: Option<GraphRevision>,
    candidate: OpenGpuiPortCandidateInput,
}

impl OpenGpuiConnectionCandidateInput {
    pub(crate) fn new(
        port: PortId,
        probe_revision: Option<GraphRevision>,
        candidate: OpenGpuiPortCandidateInput,
    ) -> Self {
        Self {
            port,
            probe_revision,
            candidate,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OpenGpuiProjectedInteractionInput {
    phase: CanvasInteractionPhase,
    generation: Option<InteractionGeneration>,
    intent: Option<ConnectionGestureBinding>,
    ticket: Option<InteractionTicketId>,
    candidate: Option<OpenGpuiConnectionCandidateInput>,
}

impl OpenGpuiProjectedInteractionInput {
    pub(crate) fn new(
        phase: CanvasInteractionPhase,
        generation: Option<InteractionGeneration>,
        intent: Option<ConnectionGestureBinding>,
        ticket: Option<InteractionTicketId>,
        candidate: Option<OpenGpuiConnectionCandidateInput>,
    ) -> Self {
        Self {
            phase,
            generation,
            intent,
            ticket,
            candidate,
        }
    }

    pub(crate) fn candidate_for_port(&self, port: PortId) -> Option<OpenGpuiPortCandidateInput> {
        self.candidate
            .as_ref()
            .filter(|candidate| candidate.port == port)
            .map(|candidate| candidate.candidate.clone())
    }

    pub(crate) fn phase(&self) -> CanvasInteractionPhase {
        self.phase
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenGpuiPortPresentationInput {
    port: PortId,
    connection: PortConnectionSummary,
    hovered: bool,
    focused: bool,
    candidate: OpenGpuiPortCandidateInput,
    pending: bool,
}

impl OpenGpuiPortPresentationInput {
    pub(crate) fn new(
        port: PortId,
        connection: PortConnectionSummary,
        hovered: bool,
        focused: bool,
        candidate: OpenGpuiPortCandidateInput,
        pending: bool,
    ) -> Self {
        Self {
            port,
            connection,
            hovered,
            focused,
            candidate,
            pending,
        }
    }

    fn materialize(&self) -> OpenGpuiPortInteractionState {
        OpenGpuiPortInteractionState::idle()
            .with_hovered(self.hovered)
            .with_connected(self.connection.is_connected())
            .with_disabled(
                !self.connection.can_start_connection() && !self.connection.can_accept_connection(),
            )
            .with_focused(self.focused)
            .with_candidate(self.candidate.materialize())
            .with_settlement(if self.pending {
                OpenGpuiSettlementState::Pending
            } else {
                OpenGpuiSettlementState::Idle
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenGpuiNodePresentationInputSignature {
    graph_revision: GraphRevision,
    binding_revision: GraphRevision,
    node: OpenGpuiNodeInteractionState,
    interaction: OpenGpuiProjectedInteractionInput,
    ports: Vec<OpenGpuiPortPresentationInput>,
}

impl OpenGpuiNodePresentationInputSignature {
    pub(crate) fn new(
        graph_revision: GraphRevision,
        binding_revision: GraphRevision,
        node: OpenGpuiNodeInteractionState,
        interaction: OpenGpuiProjectedInteractionInput,
        ports: Vec<OpenGpuiPortPresentationInput>,
    ) -> Self {
        Self {
            graph_revision,
            binding_revision,
            node,
            interaction,
            ports,
        }
    }

    pub(crate) fn semantically_eq(&self, other: &Self) -> bool {
        self.binding_revision == other.binding_revision
            && self.node == other.node
            && self.interaction == other.interaction
            && self.ports.len() == other.ports.len()
            && self
                .ports
                .iter()
                .zip(&other.ports)
                .all(|(left, right)| left.semantically_eq(right))
    }

    pub(crate) fn graph_revision(&self) -> GraphRevision {
        self.graph_revision
    }

    pub(crate) fn ordered_connection_summaries(
        &self,
    ) -> impl ExactSizeIterator<Item = (PortId, PortConnectionSummary)> + '_ {
        self.ports.iter().map(|port| (port.port, port.connection))
    }

    pub(crate) fn materialize(&self) -> OpenGpuiNodePresentationSnapshot {
        OpenGpuiNodePresentationSnapshot::new(
            self.node,
            self.ports
                .iter()
                .map(|port| (port.port, port.materialize())),
        )
    }
}

impl OpenGpuiPortPresentationInput {
    fn semantically_eq(&self, other: &Self) -> bool {
        self.port == other.port
            && port_connection_semantically_eq(self.connection, other.connection)
            && self.hovered == other.hovered
            && self.focused == other.focused
            && self.candidate == other.candidate
            && self.pending == other.pending
    }
}

fn port_connection_semantically_eq(
    left: PortConnectionSummary,
    right: PortConnectionSummary,
) -> bool {
    left.instance() == right.instance()
        && left.port() == right.port()
        && left.node() == right.node()
        && left.direction() == right.direction()
        && left.capacity() == right.capacity()
        && left.can_start_connection() == right.can_start_connection()
        && left.can_accept_connection() == right.can_accept_connection()
        && left.incident_count() == right.incident_count()
}

/// Retained, adapter-owned semantic binding passed to native Open GPUI node entities.
pub struct OpenGpuiNodeBinding {
    mount: OpenGpuiBindingMount,
    presentation: OpenGpuiNodePresentationSnapshot,
    presentation_input: Option<OpenGpuiNodePresentationInputSignature>,
    ports: BTreeMap<PortId, OpenGpuiBindingPortCapability>,
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
            self.mount.context.clone(),
            self.mount.commands.clone(),
            self.activity.clone(),
            |port| self.base_port_handle(port),
            self.presentation_input.as_ref(),
            self.presentation.clone(),
        )
    }

    /// Returns the latest immutable adapter-local node and port interaction state.
    pub fn presentation(&self) -> &OpenGpuiNodePresentationSnapshot {
        &self.presentation
    }

    pub fn activity(&self) -> OpenGpuiNodeActivityHandle {
        self.activity.renderer_handle()
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
        if !self.mount.context.mount().is_live() {
            return Ok(false);
        }
        (self.layout_invalidator)(
            self.mount.context.instance(),
            self.mount.context.mount(),
            cx,
        )
    }

    pub(crate) fn cached_port_handle(&self, port: PortId) -> Option<OpenGpuiPortHandle> {
        Some(
            self.base_port_handle(port)?.with_presentation_state(
                self.presentation
                    .port(port)
                    .cloned()
                    .unwrap_or_else(OpenGpuiPortInteractionState::idle),
            ),
        )
    }

    fn base_port_handle(&self, port: PortId) -> Option<OpenGpuiPortHandle> {
        self.ports
            .get(&port)
            .map(|capability| capability.handle.clone())
    }

    pub(crate) fn new(
        mount: OpenGpuiBindingMount,
        activity: OpenGpuiNodeActivityRegistry,
        drag_handle: OpenGpuiNodeDragHandle,
        measurement: OpenGpuiMeasurementSink,
        layout_invalidator: OpenGpuiLayoutInvalidator,
        cx: &mut Context<Self>,
    ) -> Self {
        let ports = mount
            .facts
            .port_facts
            .iter()
            .map(|fact| {
                let focus = cx.focus_handle();
                let handle = OpenGpuiPortHandle::new(
                    mount.context.instance().clone(),
                    fact.instance,
                    fact.direction,
                    Arc::clone(&fact.name),
                    measurement.clone(),
                )
                .with_focus_handle(Some(focus.clone()));
                (
                    fact.id,
                    OpenGpuiBindingPortCapability {
                        instance: fact.instance,
                        direction: fact.direction,
                        name: Arc::clone(&fact.name),
                        focus,
                        handle,
                    },
                )
            })
            .collect();
        let presentation = OpenGpuiNodePresentationSnapshot::default();
        drag_handle.set_graph_enabled(mount.facts.node_drag_enabled);
        drag_handle.set_presentation_enabled(
            !presentation.node().disabled()
                && presentation.node().lod().publishes_native_graph_geometry(),
        );
        drag_handle.set_dragging(presentation.node().dragging());
        Self {
            mount,
            presentation,
            presentation_input: None,
            ports,
            activity,
            drag_handle,
            measurement,
            layout_invalidator,
        }
    }

    pub(crate) fn advance(
        &mut self,
        facts_revision: GraphRevision,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        let outcome = self.mount.advance_from_view(facts_revision)?;
        if outcome == OpenGpuiBindingAdvance::Applied {
            self.commit_advanced_facts(cx);
        }
        Ok(outcome)
    }

    pub(crate) fn advance_from_frame(
        &mut self,
        frame: &FrameNode,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        let outcome = self.mount.advance_from_frame(frame)?;
        self.commit_advanced_facts(cx);
        Ok(outcome)
    }

    fn commit_advanced_facts(&mut self, cx: &mut Context<Self>) {
        self.drag_handle
            .set_graph_enabled(self.mount.facts.node_drag_enabled);
        self.ports
            .retain(|port, _| self.mount.cached_port_facts(*port).is_some());
        self.reconcile_port_capabilities(cx);
    }

    pub(crate) fn port_focus_handle(&self, port: PortId) -> Option<&FocusHandle> {
        self.ports.get(&port).map(|capability| &capability.focus)
    }

    pub(crate) fn port_instance(&self, port: PortId) -> Option<PortInstanceKey> {
        self.ports.get(&port).map(|capability| capability.instance)
    }

    pub(crate) fn facts_revision(&self) -> GraphRevision {
        self.mount.facts.revision
    }

    pub(crate) fn port_ids(&self) -> impl ExactSizeIterator<Item = PortId> + '_ {
        self.mount.facts.port_facts.ids()
    }

    fn reconcile_port_capabilities(&mut self, cx: &mut Context<Self>) {
        for fact in self.mount.facts.port_facts.iter() {
            match self.ports.entry(fact.id) {
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    if entry.get().matches(fact) {
                        continue;
                    }
                    let focus = if entry.get().instance == fact.instance {
                        entry.get().focus.clone()
                    } else {
                        cx.focus_handle()
                    };
                    let handle = OpenGpuiPortHandle::new(
                        self.mount.context.instance().clone(),
                        fact.instance,
                        fact.direction,
                        Arc::clone(&fact.name),
                        self.measurement.clone(),
                    )
                    .with_focus_handle(Some(focus.clone()));
                    entry.insert(OpenGpuiBindingPortCapability {
                        instance: fact.instance,
                        direction: fact.direction,
                        name: Arc::clone(&fact.name),
                        focus,
                        handle,
                    });
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let focus = cx.focus_handle();
                    let handle = OpenGpuiPortHandle::new(
                        self.mount.context.instance().clone(),
                        fact.instance,
                        fact.direction,
                        Arc::clone(&fact.name),
                        self.measurement.clone(),
                    )
                    .with_focus_handle(Some(focus.clone()));
                    entry.insert(OpenGpuiBindingPortCapability {
                        instance: fact.instance,
                        direction: fact.direction,
                        name: Arc::clone(&fact.name),
                        focus,
                        handle,
                    });
                }
            }
        }
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

    pub(crate) fn presentation_input(&self) -> Option<&OpenGpuiNodePresentationInputSignature> {
        self.presentation_input.as_ref()
    }

    pub(crate) fn set_presentation_input(&mut self, input: OpenGpuiNodePresentationInputSignature) {
        self.presentation_input = Some(input);
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenGpuiNodeBindingError {
    #[error(transparent)]
    Mount(#[from] OpenGpuiMountValidationError),
    #[error("frame node does not match the retained native instance: {node:?}")]
    ForeignFrameInstance { node: NodeId },
    #[error("frame node is missing from its graph snapshot: {node:?}")]
    MissingNode { node: NodeId },
    #[error("frame port {port:?} is missing from node {node:?}'s graph snapshot")]
    MissingPort { node: NodeId, port: PortId },
    #[error("frame port {port:?} has incoherent lifetime identity for node {node:?}")]
    IncoherentPortInstance { node: NodeId, port: PortId },
    #[error("frame port batch is incoherent for node {node:?}")]
    IncoherentPortBatch { node: NodeId },
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

#[cfg(test)]
mod tests {
    use jellyflow_core::{
        CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
        Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use jellyflow_runtime::{
        commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
        commands::TopologyPort, graph::DiagnosticSeverity, graph::DiagnosticTarget,
        graph::GraphRuntime, view::FrameRequest, view::RendererStamp, view::ViewCloseReason,
        view::ViewSession, view::ViewSessionOptions,
    };

    use super::*;

    #[test]
    fn invalid_candidate_diagnostics_participate_in_signature_and_materialization() {
        let port = PortId::from_u128(42);
        let first = OpenGpuiPortCandidateInput::invalid(&[diagnostic(
            port,
            "connect.type_mismatch",
            "Integer cannot connect to text",
        )]);
        let changed = OpenGpuiPortCandidateInput::invalid(&[diagnostic(
            port,
            "connect.capacity",
            "Input already has a connection",
        )]);

        let first_interaction = projected_candidate(port, first.clone());
        let changed_interaction = projected_candidate(port, changed.clone());
        assert_ne!(first_interaction, changed_interaction);
        let first_signature = OpenGpuiNodePresentationInputSignature::new(
            11,
            7,
            OpenGpuiNodeInteractionState::idle(),
            first_interaction,
            Vec::new(),
        );
        let changed_signature = OpenGpuiNodePresentationInputSignature::new(
            11,
            7,
            OpenGpuiNodeInteractionState::idle(),
            changed_interaction,
            Vec::new(),
        );
        assert!(
            !first_signature.semantically_eq(&changed_signature),
            "diagnostic changes must invalidate an otherwise identical presentation signature"
        );

        let first_state = first.materialize();
        let changed_state = changed.materialize();
        assert_ne!(first_state, changed_state);
        assert_eq!(
            changed_state.rejection_text(),
            Some("Input already has a connection")
        );

        let first_snapshot = OpenGpuiNodePresentationSnapshot::new(
            OpenGpuiNodeInteractionState::idle(),
            [(
                port,
                OpenGpuiPortInteractionState::idle().with_candidate(first_state),
            )],
        );
        let changed_snapshot = OpenGpuiNodePresentationSnapshot::new(
            OpenGpuiNodeInteractionState::idle(),
            [(
                port,
                OpenGpuiPortInteractionState::idle().with_candidate(changed_state),
            )],
        );
        assert_ne!(
            first_snapshot, changed_snapshot,
            "the host's final equality gate must admit a binding notification"
        );
    }

    #[test]
    fn mount_context_rejects_foreign_views_and_runtimes_before_node_validation() {
        let first_runtime = mount_runtime();
        let first_view = first_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("first mount view");
        let sibling_view = first_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("sibling mount view");
        let first_frame = mount_frame(&first_view);
        let instance = first_frame.nodes()[0].instance().clone();

        assert!(matches!(
            OpenGpuiMountContext::new(
                instance.clone(),
                sibling_view.downgrade(),
                OpenGpuiMountCapability::new(),
            ),
            Err(OpenGpuiMountValidationError::ForeignView)
        ));

        let second_runtime = mount_runtime();
        let second_view = second_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("second runtime mount view");
        assert_eq!(first_view.id().get(), second_view.id().get());
        assert!(matches!(
            OpenGpuiMountContext::new(
                instance,
                second_view.downgrade(),
                OpenGpuiMountCapability::new(),
            ),
            Err(OpenGpuiMountValidationError::ForeignView)
        ));
    }

    #[test]
    fn mount_context_permanently_rejects_retired_mount_view_and_node_lifetimes() {
        let runtime = mount_runtime();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("mount view");
        let frame = mount_frame(&view);
        let instance = frame.nodes()[0].instance().clone();
        let mount = OpenGpuiMountCapability::new();
        let context = OpenGpuiMountContext::new(instance.clone(), view.downgrade(), mount.clone())
            .expect("exact mount context");

        mount.retire();
        assert!(matches!(
            context.ensure_current(),
            Err(OpenGpuiMountValidationError::RetiredMount { node }) if node == mount_node_id()
        ));

        let live_mount = OpenGpuiMountCapability::new();
        let live_context = OpenGpuiMountContext::new(instance, view.downgrade(), live_mount)
            .expect("second exact mount context");
        let removed = runtime
            .execute(GraphCommand::topology(
                frame.graph_version(),
                TopologyEdit::new().remove_node(mount_node_id(), IncidentEdgePolicy::Reject),
            ))
            .into_result()
            .expect("remove mounted node");
        let removed_version = removed.applied().expect("node removal applies").version();
        runtime
            .execute(GraphCommand::undo(removed_version))
            .into_result()
            .expect("restore mounted node");
        assert!(matches!(
            live_context.ensure_current(),
            Err(OpenGpuiMountValidationError::RetiredIncarnation { node })
                if node == mount_node_id()
        ));

        let close_runtime = mount_runtime();
        let close_view = close_runtime
            .attach_view(ViewSessionOptions::default())
            .expect("close mount view");
        let close_instance = mount_frame(&close_view).nodes()[0].instance().clone();
        let closed_context = OpenGpuiMountContext::new(
            close_instance,
            close_view.downgrade(),
            OpenGpuiMountCapability::new(),
        )
        .expect("close mount context");
        close_view.close().expect("close exact view generation");
        assert!(matches!(
            closed_context.ensure_current(),
            Err(OpenGpuiMountValidationError::Closed {
                reason: ViewCloseReason::HostClosed,
            })
        ));
    }

    #[test]
    fn transient_refresh_failure_preserves_facts_until_an_authoritative_frame_repairs_them() {
        let runtime = mount_runtime();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("binding refresh view");
        let first_frame = mount_frame(&view);
        let first_node = &first_frame.nodes()[0];
        let context = OpenGpuiMountContext::new(
            first_node.instance().clone(),
            view.downgrade(),
            OpenGpuiMountCapability::new(),
        )
        .expect("binding refresh context");
        let mut facts =
            OpenGpuiBindingFacts::from_frame(&context, first_node).expect("initial binding facts");
        let retired_port = facts.port_facts.get(mount_port_id()).unwrap().instance;
        facts.node_drag_enabled = false;

        let removed = runtime
            .execute(GraphCommand::topology(
                first_frame.graph_version(),
                TopologyEdit::new().replace_node_ports(
                    mount_node_id(),
                    std::iter::empty::<TopologyPort>(),
                    IncidentEdgePolicy::Reject,
                ),
            ))
            .into_result()
            .expect("remove mounted port");
        let removed_version = removed.applied().expect("port removal applies").version();
        let restored = runtime
            .execute(GraphCommand::undo(removed_version))
            .into_result()
            .expect("restore mounted port");
        let restored_revision = restored
            .applied()
            .expect("port restoration applies")
            .revision();

        assert!(matches!(
            advance_binding_facts_from_view(&mut facts, &context, restored_revision, || Err(
                OpenGpuiMountValidationError::ForeignView
            ),),
            Err(OpenGpuiNodeBindingError::Mount(
                OpenGpuiMountValidationError::ForeignView,
            ))
        ));
        assert_eq!(facts.revision, first_node.binding_revision());
        assert!(!facts.node_drag_enabled);

        let outcome =
            advance_binding_facts_from_view(&mut facts, &context, restored_revision, || {
                Err(OpenGpuiMountValidationError::Session(
                    ViewSessionError::Busy,
                ))
            })
            .expect("busy refresh is retryable");
        assert_eq!(outcome, OpenGpuiBindingAdvance::Retry);
        assert_eq!(facts.revision, first_node.binding_revision());
        assert!(!facts.node_drag_enabled);
        assert_eq!(
            facts.port_facts.get(mount_port_id()).unwrap().instance,
            retired_port
        );

        let repaired_frame = mount_frame(&view);
        let repaired_node = &repaired_frame.nodes()[0];
        assert_eq!(repaired_node.binding_revision(), restored_revision);
        assert_eq!(
            advance_binding_facts_from_frame(&mut facts, &context, repaired_node)
                .expect("authoritative frame repairs facts"),
            OpenGpuiBindingAdvance::Applied
        );
        assert_eq!(facts.revision, restored_revision);
        assert!(facts.node_drag_enabled);
        assert_ne!(
            facts.port_facts.get(mount_port_id()).unwrap().instance,
            retired_port
        );
    }

    fn mount_runtime() -> GraphRuntime {
        let node = mount_node_id();
        let port = mount_port_id();
        let graph = GraphBuilder::new(GraphId::from_u128(0x0b1a_d1a6))
            .with_node(
                node,
                Node {
                    kind: NodeKindKey::new("test.binding-mount"),
                    kind_version: 1,
                    pos: CanvasPoint::default(),
                    origin: None,
                    selectable: None,
                    draggable: Some(true),
                    connectable: None,
                    deletable: None,
                    parent: None,
                    extent: None,
                    expand_parent: None,
                    size: Some(CanvasSize {
                        width: 160.0,
                        height: 80.0,
                    }),
                    hidden: false,
                    collapsed: false,
                    ports: vec![port],
                    data: serde_json::Value::Null,
                },
            )
            .with_port(
                port,
                Port {
                    node,
                    key: PortKey::new("value"),
                    dir: PortDirection::In,
                    kind: PortKind::Data,
                    capacity: PortCapacity::Single,
                    connectable: None,
                    connectable_start: None,
                    connectable_end: None,
                    ty: None,
                    data: serde_json::Value::Null,
                },
            )
            .build()
            .expect("binding mount graph");
        GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("binding mount runtime")
    }

    fn mount_frame(view: &ViewSession) -> jellyflow_runtime::view::AdapterFrame {
        view.frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint { x: -20.0, y: -20.0 },
                size: CanvasSize {
                    width: 400.0,
                    height: 300.0,
                },
            },
            RendererStamp::new("test.binding-mount", 1),
        ))
        .expect("binding mount frame")
    }

    fn mount_node_id() -> NodeId {
        NodeId::from_u128(0xb1ad_0001)
    }

    fn mount_port_id() -> PortId {
        PortId::from_u128(0xb1ad_0002)
    }

    fn projected_candidate(
        port: PortId,
        candidate: OpenGpuiPortCandidateInput,
    ) -> OpenGpuiProjectedInteractionInput {
        OpenGpuiProjectedInteractionInput::new(
            CanvasInteractionPhase::ConnectionDrag,
            None,
            None,
            None,
            Some(OpenGpuiConnectionCandidateInput::new(
                port,
                Some(7),
                candidate,
            )),
        )
    }

    fn diagnostic(port: PortId, key: &str, message: &str) -> Diagnostic {
        Diagnostic {
            key: key.to_owned(),
            severity: DiagnosticSeverity::Error,
            target: DiagnosticTarget::Port { id: port },
            message: message.to_owned(),
        }
    }
}
