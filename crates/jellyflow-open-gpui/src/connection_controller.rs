use jellyflow::core::{EdgeId, NodeGraphConnectionMode, PortId};
use jellyflow::runtime::EdgeEndpoint;
use jellyflow::runtime::{ConnectionEdit, GraphCommand, GraphRevision, TopologyEdit};

/// Stable identity of one host-owned connection interaction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpenGpuiConnectionGeneration(u64);

impl OpenGpuiConnectionGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Pure lifecycle phase of the retained connection controller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OpenGpuiConnectionPhase {
    #[default]
    Idle,
    Dragging,
    Searching,
    Committing,
    Cancelled,
}

/// Canvas-space drop point used to anchor a native create-from-wire search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenGpuiConnectionPoint {
    x: f32,
    y: f32,
}

impl OpenGpuiConnectionPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Why an active connection interaction entered its terminal cancelled phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiConnectionCancelReason {
    Escape,
    OutsideClick,
    EmptySearch,
    QueryFailed,
    CommandRejected,
    HostClosed,
    SourceRetired,
}

/// One requested connection gesture before its target and fresh revision are known.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiConnectionIntent {
    Connect {
        edge: EdgeId,
        source: PortId,
        mode: NodeGraphConnectionMode,
    },
    Reconnect {
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        fixed_port: PortId,
        mode: NodeGraphConnectionMode,
    },
}

impl OpenGpuiConnectionIntent {
    #[must_use]
    pub const fn connect(edge: EdgeId, source: PortId, mode: NodeGraphConnectionMode) -> Self {
        Self::Connect { edge, source, mode }
    }

    #[must_use]
    pub const fn reconnect(
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        fixed_port: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::Reconnect {
            edge,
            endpoint,
            fixed_port,
            mode,
        }
    }

    #[must_use]
    pub const fn source(self) -> PortId {
        match self {
            Self::Connect { source, .. } => source,
            Self::Reconnect { fixed_port, .. } => fixed_port,
        }
    }

    #[must_use]
    pub const fn mode(self) -> NodeGraphConnectionMode {
        match self {
            Self::Connect { mode, .. } | Self::Reconnect { mode, .. } => mode,
        }
    }

    #[must_use]
    pub const fn target_port(self) -> Option<PortId> {
        match self {
            Self::Connect { .. } => None,
            Self::Reconnect {
                endpoint: EdgeEndpoint::To,
                fixed_port,
                ..
            } => Some(fixed_port),
            Self::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            } => None,
        }
    }

    fn edit_for_target(self, target: PortId) -> ConnectionEdit {
        match self {
            Self::Connect { edge, source, mode } => {
                ConnectionEdit::connect(edge, source, target, mode)
            }
            Self::Reconnect {
                edge,
                endpoint,
                mode,
                ..
            } => ConnectionEdit::reconnect(edge, endpoint, target, mode),
        }
    }
}

/// Inert request for a host-owned native create-from-wire search surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenGpuiCreateFromWireRequest {
    generation: OpenGpuiConnectionGeneration,
    intent: OpenGpuiConnectionIntent,
    position: OpenGpuiConnectionPoint,
}

impl OpenGpuiCreateFromWireRequest {
    #[must_use]
    pub const fn generation(self) -> OpenGpuiConnectionGeneration {
        self.generation
    }

    #[must_use]
    pub const fn source(self) -> PortId {
        self.intent.source()
    }

    #[must_use]
    pub const fn mode(self) -> NodeGraphConnectionMode {
        self.intent.mode()
    }

    #[must_use]
    pub const fn intent(self) -> OpenGpuiConnectionIntent {
        self.intent
    }

    #[must_use]
    pub const fn position(self) -> OpenGpuiConnectionPoint {
        self.position
    }
}

/// Revision-bound command intent emitted only after host freshness hooks succeed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenGpuiConnectionCommit {
    expected_revision: GraphRevision,
    edit: ConnectionEdit,
}

impl OpenGpuiConnectionCommit {
    #[must_use]
    pub const fn edit(&self) -> ConnectionEdit {
        self.edit
    }

    #[must_use]
    pub fn into_command(self) -> GraphCommand {
        GraphCommand::connection(self.expected_revision, self.edit)
    }

    /// Appends the selected node creation before this connection in one atomic topology command.
    #[must_use]
    pub fn into_topology_command(self, creation: TopologyEdit) -> GraphCommand {
        GraphCommand::topology(self.expected_revision, creation.connection(self.edit))
    }
}

/// Rejection that leaves the current interaction active and graph state untouched.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OpenGpuiConnectionCommitRejection {
    #[error("connection interaction generation is stale")]
    StaleGeneration {
        current: Option<OpenGpuiConnectionGeneration>,
        received: OpenGpuiConnectionGeneration,
    },
    #[error("connection commit requires the dragging phase")]
    InvalidPhase { phase: OpenGpuiConnectionPhase },
    #[error("the current graph revision is unavailable")]
    RevisionUnavailable,
    #[error("fresh geometry is unavailable for port {port:?}")]
    GeometryUnavailable { port: PortId },
}

/// Result of applying a generation-bound lifecycle event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenGpuiConnectionTransition {
    Applied,
    IgnoredStale {
        current: Option<OpenGpuiConnectionGeneration>,
        received: OpenGpuiConnectionGeneration,
    },
    IgnoredPhase {
        phase: OpenGpuiConnectionPhase,
    },
}

#[derive(Clone, Copy, Debug)]
struct ActiveConnection {
    generation: OpenGpuiConnectionGeneration,
    intent: OpenGpuiConnectionIntent,
    phase: OpenGpuiConnectionPhase,
}

/// Host-owned pure state machine for retained native connection interactions.
///
/// The controller never reads a graph, dispatches a command, or owns framework entities. The host
/// supplies fresh revision and geometry hooks at commit time and executes the returned command.
#[derive(Debug)]
pub struct OpenGpuiConnectionController {
    next_generation: Option<u64>,
    active: Option<ActiveConnection>,
}

impl Default for OpenGpuiConnectionController {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenGpuiConnectionController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_generation: Some(1),
            active: None,
        }
    }

    #[must_use]
    pub fn phase(&self) -> OpenGpuiConnectionPhase {
        self.active
            .map_or(OpenGpuiConnectionPhase::Idle, |active| active.phase)
    }

    #[must_use]
    pub fn generation(&self) -> Option<OpenGpuiConnectionGeneration> {
        self.active.map(|active| active.generation)
    }

    #[must_use]
    pub fn target_port(&self) -> Option<PortId> {
        self.active.and_then(|active| active.intent.target_port())
    }

    pub fn begin_drag(
        &mut self,
        intent: OpenGpuiConnectionIntent,
    ) -> Result<OpenGpuiConnectionGeneration, OpenGpuiConnectionBeginRejection> {
        if let Some(active) = self.active {
            return Err(OpenGpuiConnectionBeginRejection::AlreadyActive {
                generation: active.generation,
                phase: active.phase,
            });
        }
        let next = self
            .next_generation
            .ok_or(OpenGpuiConnectionBeginRejection::GenerationExhausted)?;
        let generation = OpenGpuiConnectionGeneration(next);
        self.next_generation = next.checked_add(1);
        self.active = Some(ActiveConnection {
            generation,
            intent,
            phase: OpenGpuiConnectionPhase::Dragging,
        });
        Ok(generation)
    }

    pub fn request_create_from_wire(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        position: OpenGpuiConnectionPoint,
    ) -> Result<OpenGpuiCreateFromWireRequest, OpenGpuiCreateFromWireRejection> {
        if !position.is_finite() {
            return Err(OpenGpuiCreateFromWireRejection::InvalidPosition);
        }
        let active = self
            .active_for(generation)
            .map_err(OpenGpuiCreateFromWireRejection::from)?;
        if active.phase != OpenGpuiConnectionPhase::Dragging {
            return Err(OpenGpuiCreateFromWireRejection::InvalidPhase {
                phase: active.phase,
            });
        }
        active.phase = OpenGpuiConnectionPhase::Searching;
        Ok(OpenGpuiCreateFromWireRequest {
            generation,
            intent: active.intent,
            position,
        })
    }

    pub fn commit_to_port(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
        fresh_revision: impl FnOnce() -> Option<GraphRevision>,
        has_fresh_geometry: impl FnOnce() -> bool,
    ) -> Result<OpenGpuiConnectionCommit, OpenGpuiConnectionCommitRejection> {
        let intent = self.intent_for_dragging(generation)?;
        let commit = Self::prepare_commit(
            target,
            intent.edit_for_target(target),
            fresh_revision,
            has_fresh_geometry,
        )?;
        self.mark_committing(generation);
        Ok(commit)
    }

    pub fn commit_create_from_wire(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        target: PortId,
        query_revision: GraphRevision,
    ) -> Result<OpenGpuiConnectionCommit, OpenGpuiConnectionCommitRejection> {
        let active = self
            .active_for(generation)
            .map_err(OpenGpuiConnectionCommitRejection::from)?;
        if active.phase != OpenGpuiConnectionPhase::Searching {
            return Err(OpenGpuiConnectionCommitRejection::InvalidPhase {
                phase: active.phase,
            });
        }
        let commit = OpenGpuiConnectionCommit {
            expected_revision: query_revision,
            edit: active.intent.edit_for_target(target),
        };
        active.phase = OpenGpuiConnectionPhase::Committing;
        Ok(commit)
    }

    pub fn finish_commit(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
    ) -> OpenGpuiConnectionTransition {
        let transition = self.transition_phase(
            generation,
            OpenGpuiConnectionPhase::Committing,
            OpenGpuiConnectionPhase::Idle,
        );
        if transition == OpenGpuiConnectionTransition::Applied {
            self.active = None;
        }
        transition
    }

    pub fn reject_commit(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
    ) -> OpenGpuiConnectionTransition {
        self.transition_phase(
            generation,
            OpenGpuiConnectionPhase::Committing,
            OpenGpuiConnectionPhase::Dragging,
        )
    }

    pub fn reject_create_from_wire_commit(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
    ) -> OpenGpuiConnectionTransition {
        self.transition_phase(
            generation,
            OpenGpuiConnectionPhase::Committing,
            OpenGpuiConnectionPhase::Searching,
        )
    }

    /// Terminally aborts an interaction, including one waiting on a command receipt.
    ///
    /// The matching generation is removed immediately so a late receipt is stale and a new drag
    /// may begin without waiting for command execution to settle.
    pub fn abort(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        _reason: OpenGpuiConnectionCancelReason,
    ) -> OpenGpuiConnectionTransition {
        match self.active_for(generation) {
            Ok(_) => {}
            Err(stale) => return stale.into_transition(),
        }
        self.active = None;
        OpenGpuiConnectionTransition::Applied
    }

    fn intent_for_dragging(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
    ) -> Result<OpenGpuiConnectionIntent, OpenGpuiConnectionCommitRejection> {
        let active = self
            .active_for(generation)
            .map_err(OpenGpuiConnectionCommitRejection::from)?;
        if active.phase != OpenGpuiConnectionPhase::Dragging {
            return Err(OpenGpuiConnectionCommitRejection::InvalidPhase {
                phase: active.phase,
            });
        }
        Ok(active.intent)
    }

    fn prepare_commit(
        geometry_port: PortId,
        edit: ConnectionEdit,
        fresh_revision: impl FnOnce() -> Option<GraphRevision>,
        has_fresh_geometry: impl FnOnce() -> bool,
    ) -> Result<OpenGpuiConnectionCommit, OpenGpuiConnectionCommitRejection> {
        let expected_revision =
            fresh_revision().ok_or(OpenGpuiConnectionCommitRejection::RevisionUnavailable)?;
        if !has_fresh_geometry() {
            return Err(OpenGpuiConnectionCommitRejection::GeometryUnavailable {
                port: geometry_port,
            });
        }
        Ok(OpenGpuiConnectionCommit {
            expected_revision,
            edit,
        })
    }

    fn mark_committing(&mut self, generation: OpenGpuiConnectionGeneration) {
        let active = self
            .active_for(generation)
            .expect("the generation was validated before commit preparation");
        active.phase = OpenGpuiConnectionPhase::Committing;
    }

    fn transition_phase(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
        from: OpenGpuiConnectionPhase,
        to: OpenGpuiConnectionPhase,
    ) -> OpenGpuiConnectionTransition {
        let active = match self.active_for(generation) {
            Ok(active) => active,
            Err(stale) => return stale.into_transition(),
        };
        if active.phase != from {
            return OpenGpuiConnectionTransition::IgnoredPhase {
                phase: active.phase,
            };
        }
        active.phase = to;
        OpenGpuiConnectionTransition::Applied
    }

    fn active_for(
        &mut self,
        generation: OpenGpuiConnectionGeneration,
    ) -> Result<&mut ActiveConnection, StaleConnectionGeneration> {
        let current = self.active.map(|active| active.generation);
        match self.active.as_mut() {
            Some(active) if active.generation == generation => Ok(active),
            _ => Err(StaleConnectionGeneration {
                current,
                received: generation,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OpenGpuiConnectionBeginRejection {
    #[error("a connection interaction is already active")]
    AlreadyActive {
        generation: OpenGpuiConnectionGeneration,
        phase: OpenGpuiConnectionPhase,
    },
    #[error("connection interaction generation is exhausted")]
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum OpenGpuiCreateFromWireRejection {
    #[error("connection interaction generation is stale")]
    StaleGeneration {
        current: Option<OpenGpuiConnectionGeneration>,
        received: OpenGpuiConnectionGeneration,
    },
    #[error("create-from-wire requires the dragging phase")]
    InvalidPhase { phase: OpenGpuiConnectionPhase },
    #[error("create-from-wire position must be finite")]
    InvalidPosition,
}

#[derive(Clone, Copy, Debug)]
struct StaleConnectionGeneration {
    current: Option<OpenGpuiConnectionGeneration>,
    received: OpenGpuiConnectionGeneration,
}

impl StaleConnectionGeneration {
    fn into_transition(self) -> OpenGpuiConnectionTransition {
        OpenGpuiConnectionTransition::IgnoredStale {
            current: self.current,
            received: self.received,
        }
    }
}

impl From<StaleConnectionGeneration> for OpenGpuiConnectionCommitRejection {
    fn from(value: StaleConnectionGeneration) -> Self {
        Self::StaleGeneration {
            current: value.current,
            received: value.received,
        }
    }
}

impl From<StaleConnectionGeneration> for OpenGpuiCreateFromWireRejection {
    fn from(value: StaleConnectionGeneration) -> Self {
        Self::StaleGeneration {
            current: value.current,
            received: value.received,
        }
    }
}

#[cfg(test)]
mod tests {
    use jellyflow::core::{EdgeId, NodeGraphConnectionMode, PortId};
    use jellyflow::runtime::EdgeEndpoint;
    use jellyflow::runtime::{ConnectionEdit, GraphCommand, TopologyEdit};

    use super::*;

    fn port(value: u128) -> PortId {
        PortId::from_u128(value)
    }

    fn edge(value: u128) -> EdgeId {
        EdgeId::from_u128(value)
    }

    #[test]
    fn drag_search_commit_and_finish_are_explicit_phases() {
        let mut controller = OpenGpuiConnectionController::new();
        let drag = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(10),
                port(1),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);

        let search = controller
            .request_create_from_wire(drag, OpenGpuiConnectionPoint::new(80.0, 120.0))
            .unwrap();
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Searching);
        assert_eq!(search.generation(), drag);
        assert_eq!(search.source(), port(1));
        assert_eq!(search.mode(), NodeGraphConnectionMode::Strict);
        assert_eq!(search.position(), OpenGpuiConnectionPoint::new(80.0, 120.0));

        let commit = controller
            .commit_create_from_wire(search.generation(), port(2), 7)
            .unwrap();
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Committing);
        assert_eq!(commit.clone().into_command().expected_revision(), 7);
        assert!(matches!(
            commit.edit(),
            ConnectionEdit::Connect { edge: id, from, to, mode }
                if id == edge(10)
                    && from == port(1)
                    && to == port(2)
                    && mode == NodeGraphConnectionMode::Strict
        ));
        assert_eq!(
            controller.finish_commit(drag),
            OpenGpuiConnectionTransition::Applied
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Idle);
    }

    #[test]
    fn stale_generation_cannot_abort_or_commit_a_new_drag() {
        let mut controller = OpenGpuiConnectionController::new();
        let old = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(10),
                port(1),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        assert_eq!(
            controller.abort(old, OpenGpuiConnectionCancelReason::Escape),
            OpenGpuiConnectionTransition::Applied
        );
        let current = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(11),
                port(3),
                NodeGraphConnectionMode::Loose,
            ))
            .unwrap();
        assert_ne!(old, current);

        assert_eq!(
            controller.abort(old, OpenGpuiConnectionCancelReason::HostClosed),
            OpenGpuiConnectionTransition::IgnoredStale {
                current: Some(current),
                received: old,
            }
        );
        assert!(matches!(
            controller.commit_to_port(old, port(4), || Some(8), || true),
            Err(OpenGpuiConnectionCommitRejection::StaleGeneration {
                current: Some(generation),
                received,
            }) if generation == current && received == old
        ));
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);
    }

    #[test]
    fn explicit_reconnect_preserves_edge_identity_and_endpoint() {
        let mut controller = OpenGpuiConnectionController::new();
        let generation = controller
            .begin_drag(OpenGpuiConnectionIntent::reconnect(
                edge(42),
                EdgeEndpoint::To,
                port(9),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();

        let commit = controller
            .commit_to_port(generation, port(10), || Some(13), || true)
            .unwrap();
        assert!(matches!(
            commit.edit(),
            ConnectionEdit::Reconnect { edge: id, endpoint, new_port, mode }
                if id == edge(42)
                    && endpoint == EdgeEndpoint::To
                    && new_port == port(10)
                    && mode == NodeGraphConnectionMode::Strict
        ));
    }

    #[test]
    fn connect_commit_keeps_its_new_edge_identity_for_runtime_capacity_replacement() {
        let mut controller = OpenGpuiConnectionController::new();
        let generation = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(99),
                port(7),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        let commit = controller
            .commit_to_port(generation, port(8), || Some(21), || true)
            .unwrap();
        assert!(matches!(
            commit.edit(),
            ConnectionEdit::Connect { edge: id, from, to, mode }
                if id == edge(99)
                    && from == port(7)
                    && to == port(8)
                    && mode == NodeGraphConnectionMode::Strict
        ));
        let command: GraphCommand = commit.into_command();
        assert_eq!(command.expected_revision(), 21);
    }

    #[test]
    fn create_from_wire_commit_uses_the_original_intent_and_query_revision() {
        let mut controller = OpenGpuiConnectionController::new();
        let generation = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(99),
                port(7),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        let request = controller
            .request_create_from_wire(generation, OpenGpuiConnectionPoint::new(8.0, 12.0))
            .unwrap();
        assert_eq!(request.intent().source(), port(7));

        let commit = controller
            .commit_create_from_wire(request.generation(), port(8), 21)
            .unwrap();
        assert!(matches!(
            commit.edit(),
            ConnectionEdit::Connect { edge: id, from, to, mode }
                if id == edge(99)
                    && from == port(7)
                    && to == port(8)
                    && mode == NodeGraphConnectionMode::Strict
        ));
        assert_eq!(commit.clone().into_command().expected_revision(), 21);
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Committing);
        assert_eq!(
            controller.reject_create_from_wire_commit(generation),
            OpenGpuiConnectionTransition::Applied
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Searching);
    }

    #[test]
    fn create_from_wire_commit_can_append_connection_to_one_topology_command() {
        let mut controller = OpenGpuiConnectionController::new();
        let generation = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(99),
                port(7),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        controller
            .request_create_from_wire(generation, OpenGpuiConnectionPoint::new(8.0, 12.0))
            .unwrap();

        let command = controller
            .commit_create_from_wire(generation, port(8), 21)
            .unwrap()
            .into_topology_command(TopologyEdit::new());

        assert_eq!(command.expected_revision(), 21);
        let footprint = command.declared_footprint();
        assert!(footprint.edges.contains(&edge(99)));
        assert!(footprint.ports.contains(&port(7)));
        assert!(footprint.ports.contains(&port(8)));
    }

    #[test]
    fn committing_generation_can_be_aborted_without_blocking_the_next_drag() {
        let mut controller = OpenGpuiConnectionController::new();
        let aborted = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(99),
                port(7),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();
        controller
            .request_create_from_wire(aborted, OpenGpuiConnectionPoint::new(8.0, 12.0))
            .unwrap();
        controller
            .commit_create_from_wire(aborted, port(8), 21)
            .unwrap();
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Committing);

        assert_eq!(
            controller.abort(aborted, OpenGpuiConnectionCancelReason::SourceRetired),
            OpenGpuiConnectionTransition::Applied
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Idle);

        let current = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(100),
                port(9),
                NodeGraphConnectionMode::Loose,
            ))
            .unwrap();
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);
        assert_eq!(
            controller.reject_create_from_wire_commit(aborted),
            OpenGpuiConnectionTransition::IgnoredStale {
                current: Some(current),
                received: aborted,
            }
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);
    }

    #[test]
    fn commit_requires_fresh_revision_and_target_geometry() {
        let mut controller = OpenGpuiConnectionController::new();
        let generation = controller
            .begin_drag(OpenGpuiConnectionIntent::connect(
                edge(10),
                port(1),
                NodeGraphConnectionMode::Strict,
            ))
            .unwrap();

        assert_eq!(
            controller.commit_to_port(generation, port(2), || None, || true),
            Err(OpenGpuiConnectionCommitRejection::RevisionUnavailable)
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);
        assert_eq!(
            controller.commit_to_port(generation, port(2), || Some(3), || false),
            Err(OpenGpuiConnectionCommitRejection::GeometryUnavailable { port: port(2) })
        );
        assert_eq!(controller.phase(), OpenGpuiConnectionPhase::Dragging);
    }
}
