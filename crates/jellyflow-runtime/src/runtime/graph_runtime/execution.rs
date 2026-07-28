use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::graph_kernel::{
    GraphHistory, GraphMutationError, GraphMutationFootprint, GraphTransaction,
};
use jellyflow_core::core::{Graph, validate_graph_structural};

use crate::profile::{ApplyPipelineError, validate_profile_graph};
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::commands::{
    GraphCommand, GraphCommandConflict, GraphCommandExecutionKind, GraphCommandIdentity,
    GraphCommandNoop, GraphCommandOutcome, GraphCommandPlanningError, GraphCommandPlanningWork,
    GraphCommandRejection, PlannedGraphCommand, plan_graph_command,
};
use crate::runtime::middleware::GraphRuntimeMiddlewareSnapshot;
use crate::runtime::transaction_gate::GraphTransactionGate;
use crate::runtime::view_session::{ViewGeneration, ViewShared};
use crate::schema::NodeCatalog;

use super::impact::InstalledTransactionImpact;
use super::{
    EdgeIncarnation, EdgeIncarnationEntry, GraphRuntimeError, GraphSnapshot, GraphVersion,
    HistoryState, NodeIncarnation, PortIncarnation, PortIncarnationEntry, RuntimePatch,
    RuntimeShared, RuntimeState,
};

pub(super) struct MutationGuard<'a>(&'a Cell<bool>);

impl<'a> MutationGuard<'a> {
    pub(super) fn acquire(runtime: &'a RuntimeShared) -> Result<Self, GraphRuntimeError> {
        if runtime.notifications.draining.get() || runtime.mutating.replace(true) {
            return Err(GraphRuntimeError::ReentrantMutation);
        }
        Ok(Self(&runtime.mutating))
    }
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

pub(super) struct NotificationDrainGuard<'a>(pub(super) &'a Cell<bool>);

impl Drop for NotificationDrainGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[derive(Debug, Clone, Copy)]
enum HistoryDirection {
    Undo,
    Redo,
}

impl HistoryDirection {
    fn prepare(self, history: &GraphHistory) -> Option<GraphTransaction> {
        match self {
            Self::Undo => history.prepare_undo(),
            Self::Redo => history.prepare_redo(),
        }
    }

    fn commit(self, history: &mut GraphHistory) {
        match self {
            Self::Undo => history.commit_undo(),
            Self::Redo => history.commit_redo(),
        }
    }
}

pub(super) enum CommandExecution {
    Outcome(GraphCommandOutcome),
    Committed {
        identity: GraphCommandIdentity,
        patch: RuntimePatch,
        sessions: Vec<(Rc<ViewShared>, ViewGeneration)>,
        footprint: GraphMutationFootprint,
        planning_work: GraphCommandPlanningWork,
    },
}

pub(super) fn execute_command_locked(
    state: &mut RuntimeState,
    catalog: &NodeCatalog,
    command: &GraphCommand,
) -> Result<CommandExecution, GraphRuntimeError> {
    let current_version = GraphVersion::new(state.provenance, state.revision);
    if command.expected_version().provenance() != state.provenance {
        return Err(GraphRuntimeError::ForeignCommandRuntime {
            command: command.expected_version().provenance(),
            runtime: state.provenance,
        });
    }
    let mut declared_footprint = command.conflict_footprint(&state.graph, &state.index);
    if state.profile.is_some() {
        declared_footprint.touch_graph();
    }
    if current_version != command.expected_version() {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Conflict(
            GraphCommandConflict::new(
                command.identity().clone(),
                command.expected_version(),
                GraphSnapshot::new(
                    state.provenance,
                    state.graph.clone(),
                    state.revision,
                    state.document_epoch,
                    state.incarnations.clone(),
                    state.edge_incarnations.clone(),
                    state.port_incarnations.clone(),
                ),
                declared_footprint,
            ),
        )));
    }

    match command.execution_kind() {
        GraphCommandExecutionKind::Undo => {
            return execute_history_command_locked(state, command, HistoryDirection::Undo);
        }
        GraphCommandExecutionKind::Redo => {
            return execute_history_command_locked(state, command, HistoryDirection::Redo);
        }
        GraphCommandExecutionKind::Mutation => {}
    }

    let planned = plan_graph_command(
        &state.graph,
        &state.index,
        catalog,
        command,
        state.profile.as_deref(),
        &state.policy,
    );
    let PlannedGraphCommand {
        transaction,
        footprint,
        graph,
        planning_work,
    } = match planned {
        Ok(planned) => planned,
        Err(error) => {
            return Ok(CommandExecution::Outcome(GraphCommandOutcome::Rejected(
                command_rejection_from_planning(command.identity().clone(), current_version, error),
            )));
        }
    };
    if let Err(error) = run_before_command_middleware(state, command) {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Rejected(
            command_rejection_from_apply(command.identity().clone(), current_version, error),
        )));
    }
    if transaction.is_empty() {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Noop(
            GraphCommandNoop::new(
                command.identity().clone(),
                current_version,
                footprint,
                planning_work,
            ),
        )));
    }

    let (patch, sessions) =
        install_incremental(state, graph, transaction, command.records_history())?;

    Ok(CommandExecution::Committed {
        identity: command.identity().clone(),
        patch,
        sessions,
        footprint,
        planning_work,
    })
}

fn execute_history_command_locked(
    state: &mut RuntimeState,
    command: &GraphCommand,
    direction: HistoryDirection,
) -> Result<CommandExecution, GraphRuntimeError> {
    let current_version = GraphVersion::new(state.provenance, state.revision);
    if let Err(error) = run_before_command_middleware(state, command) {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Rejected(
            command_rejection_from_apply(command.identity().clone(), current_version, error),
        )));
    }
    state
        .revision
        .checked_add(1)
        .ok_or(GraphRuntimeError::RevisionExhausted)?;
    let Some(transaction) = direction.prepare(&state.history) else {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Noop(
            GraphCommandNoop::new(
                command.identity().clone(),
                current_version,
                GraphMutationFootprint::new(),
                GraphCommandPlanningWork::empty(),
            ),
        )));
    };
    let replayed = match GraphTransactionGate::normalize_and_validate(transaction) {
        Ok(replayed) => replayed,
        Err(error) => {
            return Ok(CommandExecution::Outcome(GraphCommandOutcome::Rejected(
                command_rejection_from_apply(command.identity().clone(), current_version, error),
            )));
        }
    };
    if replayed.is_empty() {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Noop(
            GraphCommandNoop::new(
                command.identity().clone(),
                current_version,
                GraphMutationFootprint::new(),
                GraphCommandPlanningWork::empty(),
            ),
        )));
    }

    let mut after = (*state.graph).clone();
    let replay_result: Result<(), ApplyPipelineError> = (|| {
        replayed.apply_prevalidated_batch(&mut after)?;
        if let Some(profile) = state.profile.as_deref() {
            validate_profile_graph(profile, &after)?;
        }
        Ok(())
    })();
    if let Err(error) = replay_result {
        return Ok(CommandExecution::Outcome(GraphCommandOutcome::Rejected(
            command_rejection_from_apply(command.identity().clone(), current_version, error),
        )));
    }

    let footprint = replayed.footprint();
    let (patch, sessions) = install_incremental(state, after, replayed, false)?;
    direction.commit(&mut state.history);
    Ok(CommandExecution::Committed {
        identity: command.identity().clone(),
        patch,
        sessions,
        footprint,
        planning_work: GraphCommandPlanningWork::empty(),
    })
}

fn run_before_command_middleware(
    state: &mut RuntimeState,
    command: &GraphCommand,
) -> Result<(), ApplyPipelineError> {
    if state.middleware.is_none() {
        return Ok(());
    }
    let snapshot = middleware_snapshot(state);
    state
        .middleware
        .as_deref_mut()
        .expect("middleware presence was checked before snapshot creation")
        .before_command(snapshot, command)
}

pub(super) fn command_rejection_from_planning(
    identity: GraphCommandIdentity,
    version: GraphVersion,
    error: GraphCommandPlanningError,
) -> GraphCommandRejection {
    if let GraphCommandPlanningError::RuleRejected {
        message,
        diagnostics,
    } = error
    {
        return GraphCommandRejection::new(identity, version, message, diagnostics);
    }
    if let GraphCommandPlanningError::Profile(error) = error {
        return command_rejection_from_apply(identity, version, error);
    }
    let (key, target) = planning_error_diagnostic(&error);
    let message = error.to_string();
    GraphCommandRejection::new(
        identity,
        version,
        message.clone(),
        vec![Diagnostic::error(key, target, message)],
    )
}

fn planning_error_diagnostic(
    error: &GraphCommandPlanningError,
) -> (&'static str, DiagnosticTarget) {
    match error {
        GraphCommandPlanningError::Mutation(error) => mutation_error_diagnostic(error),
        GraphCommandPlanningError::MissingNode(id) => {
            ("topology.node_missing", DiagnosticTarget::Node { id: *id })
        }
        GraphCommandPlanningError::DuplicateNodeGeometry(id) => (
            "geometry.node_duplicate",
            DiagnosticTarget::Node { id: *id },
        ),
        GraphCommandPlanningError::MissingPort(id) => {
            ("topology.port_missing", DiagnosticTarget::Port { id: *id })
        }
        GraphCommandPlanningError::DuplicatePort(id) => (
            "topology.port_duplicate",
            DiagnosticTarget::Port { id: *id },
        ),
        GraphCommandPlanningError::PortOwnerMismatch { port, .. } => (
            "topology.port_owner_mismatch",
            DiagnosticTarget::Port { id: *port },
        ),
        GraphCommandPlanningError::PortConnected(id) => (
            "topology.port_connected",
            DiagnosticTarget::Port { id: *id },
        ),
        GraphCommandPlanningError::NodeConnected(id) => (
            "topology.node_connected",
            DiagnosticTarget::Node { id: *id },
        ),
        GraphCommandPlanningError::NodeLifecycleConflict(id) => (
            "topology.node_lifecycle_conflict",
            DiagnosticTarget::Node { id: *id },
        ),
        GraphCommandPlanningError::NonCanonicalNodeKind { node, .. } => (
            "topology.node_kind_not_canonical",
            DiagnosticTarget::Node { id: *node },
        ),
        GraphCommandPlanningError::PortLifecycleConflict(id) => (
            "topology.port_lifecycle_conflict",
            DiagnosticTarget::Port { id: *id },
        ),
        GraphCommandPlanningError::EdgeLifecycleConflict(id) => (
            "topology.edge_lifecycle_conflict",
            DiagnosticTarget::Edge { id: *id },
        ),
        GraphCommandPlanningError::SymbolAlreadyExists(id) => (
            "symbol.already_exists",
            DiagnosticTarget::Symbol { id: *id },
        ),
        GraphCommandPlanningError::MissingSymbol(id) => {
            ("symbol.missing", DiagnosticTarget::Symbol { id: *id })
        }
        GraphCommandPlanningError::SymbolReferenced { symbol, .. } => (
            "symbol.referenced",
            DiagnosticTarget::Symbol { id: *symbol },
        ),
        GraphCommandPlanningError::SymbolLifecycleConflict(id) => (
            "symbol.lifecycle_conflict",
            DiagnosticTarget::Symbol { id: *id },
        ),
        GraphCommandPlanningError::MissingProfileTypeDependency(id) => (
            "profile.port_type_dependency_missing",
            DiagnosticTarget::Port { id: *id },
        ),
        GraphCommandPlanningError::Apply(_) => {
            ("graph.command.invalid_operation", DiagnosticTarget::Graph)
        }
        GraphCommandPlanningError::RuleRejected { .. } | GraphCommandPlanningError::Profile(_) => {
            unreachable!("structured planning errors are handled before fallback mapping")
        }
    }
}

fn mutation_error_diagnostic(error: &GraphMutationError) -> (&'static str, DiagnosticTarget) {
    match error {
        GraphMutationError::NodeAlreadyExists(id) => (
            "topology.node_already_exists",
            DiagnosticTarget::Node { id: *id },
        ),
        GraphMutationError::PortAlreadyExists(id) => (
            "topology.port_already_exists",
            DiagnosticTarget::Port { id: *id },
        ),
        GraphMutationError::MissingPort(id) => {
            ("topology.port_missing", DiagnosticTarget::Port { id: *id })
        }
        GraphMutationError::MissingEdge(id) => {
            ("topology.edge_missing", DiagnosticTarget::Edge { id: *id })
        }
        GraphMutationError::MissingGroup(_) | GraphMutationError::MissingBinding(_) => {
            ("topology.related_fact_invalid", DiagnosticTarget::Graph)
        }
    }
}

fn command_rejection_from_apply(
    identity: GraphCommandIdentity,
    version: GraphVersion,
    error: ApplyPipelineError,
) -> GraphCommandRejection {
    match error {
        ApplyPipelineError::Rejected {
            message,
            diagnostics,
        } => GraphCommandRejection::new(identity, version, message, diagnostics),
        error => {
            let message = error.to_string();
            GraphCommandRejection::new(
                identity,
                version,
                message.clone(),
                vec![Diagnostic::error(
                    "graph.command.apply_failed",
                    DiagnosticTarget::Graph,
                    message,
                )],
            )
        }
    }
}

pub(super) fn middleware_snapshot(state: &RuntimeState) -> GraphRuntimeMiddlewareSnapshot {
    GraphRuntimeMiddlewareSnapshot::new(
        GraphSnapshot::new(
            state.provenance,
            state.graph.clone(),
            state.revision,
            state.document_epoch,
            state.incarnations.clone(),
            state.edge_incarnations.clone(),
            state.port_incarnations.clone(),
        ),
        HistoryState {
            undo_len: state.history.undo_len(),
            redo_len: state.history.redo_len(),
        },
    )
}

pub(super) fn install_incremental(
    state: &mut RuntimeState,
    graph: Graph,
    committed: GraphTransaction,
    record_history: bool,
) -> Result<(RuntimePatch, Vec<(Rc<ViewShared>, ViewGeneration)>), GraphRuntimeError> {
    let to_revision = state
        .revision
        .checked_add(1)
        .ok_or(GraphRuntimeError::RevisionExhausted)?;
    let before = state.graph.clone();
    let InstalledTransactionImpact {
        facts,
        session,
        index,
        identities,
        edge_identities,
        port_identities,
    } = InstalledTransactionImpact::derive(&before, &graph, &state.index, &committed);
    let mut next_incarnation = state.next_incarnation;
    let mut incarnation_delta = BTreeMap::new();
    for node in &identities.retired {
        incarnation_delta.insert(*node, None);
    }
    for node in &identities.added {
        incarnation_delta.insert(*node, Some(allocate_incarnation(&mut next_incarnation)?));
    }
    let mut incarnations = state.incarnations.clone();
    for node in &identities.retired {
        incarnations.remove_mut(node);
    }
    for (node, incarnation) in &incarnation_delta {
        if let Some(incarnation) = incarnation {
            incarnations.insert_mut(*node, *incarnation);
        }
    }
    let mut edge_incarnations = state.edge_incarnations.clone();
    if !edge_identities.retired.is_empty() || !edge_identities.added.is_empty() {
        for edge in &edge_identities.retired {
            edge_incarnations.remove_mut(edge);
        }
        for edge in &edge_identities.added {
            edge_incarnations.insert_mut(
                *edge,
                EdgeIncarnationEntry(allocate_edge_incarnation(&mut next_incarnation)?),
            );
        }
    }
    let mut port_incarnations = state.port_incarnations.clone();
    if !port_identities.retired.is_empty() || !port_identities.added.is_empty() {
        for port in &port_identities.retired {
            port_incarnations.remove_mut(port);
        }
        for port in &port_identities.added {
            port_incarnations.insert_mut(
                *port,
                PortIncarnationEntry(allocate_port_incarnation(&mut next_incarnation)?),
            );
        }
    }

    let graph = Arc::new(graph);
    let snapshot = GraphSnapshot::new(
        state.provenance,
        graph.clone(),
        to_revision,
        state.document_epoch,
        incarnations.clone(),
        edge_incarnations.clone(),
        port_incarnations.clone(),
    );
    let patch = RuntimePatch::incremental(
        state.revision,
        to_revision,
        state.document_epoch,
        snapshot,
        facts,
        session,
        incarnation_delta.clone(),
    );
    state.index.apply_impact(&graph, &index);
    state.graph = graph;
    state.revision = to_revision;
    state.incarnations = incarnations;
    state.edge_incarnations = edge_incarnations;
    state.port_incarnations = port_incarnations;
    state.next_incarnation = next_incarnation;
    if record_history {
        state.history.record(committed);
    }
    let sessions = collect_live_sessions(state);
    Ok((patch, sessions))
}

pub(super) fn collect_live_sessions(
    state: &mut RuntimeState,
) -> Vec<(Rc<ViewShared>, ViewGeneration)> {
    let mut dead = Vec::new();
    let mut live = Vec::new();
    for (id, session) in &state.sessions {
        let Some(session) = session.upgrade() else {
            dead.push(*id);
            continue;
        };
        let Some(generation) = session.open_generation() else {
            dead.push(*id);
            continue;
        };
        live.push((session, generation));
    }
    for id in dead {
        state.sessions.remove(&id);
    }
    live
}

pub(super) fn allocate_incarnation(next: &mut u64) -> Result<NodeIncarnation, GraphRuntimeError> {
    allocate_identity(next).map(NodeIncarnation)
}

pub(super) fn allocate_edge_incarnation(
    next: &mut u64,
) -> Result<EdgeIncarnation, GraphRuntimeError> {
    allocate_identity(next).map(EdgeIncarnation)
}

pub(super) fn allocate_port_incarnation(
    next: &mut u64,
) -> Result<PortIncarnation, GraphRuntimeError> {
    allocate_identity(next).map(PortIncarnation)
}

fn allocate_identity(next: &mut u64) -> Result<u64, GraphRuntimeError> {
    let value = *next;
    *next = next
        .checked_add(1)
        .ok_or(GraphRuntimeError::IdentityExhausted)?;
    Ok(value)
}

pub(super) fn validate_graph(graph: &Graph) -> Result<(), GraphRuntimeError> {
    let report = validate_graph_structural(graph);
    if report.is_ok() {
        Ok(())
    } else {
        Err(GraphRuntimeError::InvalidGraph {
            errors: report.into_errors(),
        })
    }
}
