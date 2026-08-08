use std::collections::BTreeSet;

use jellyflow_core::core::{
    EdgeId, EdgeKind, Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{DefaultTypeCompatibility, TypeCompatibility, TypeCompatibilityResult};

use crate::rules::{Diagnostic, DiagnosticTarget, EdgeEndpoint};
use crate::runtime::commands::ConnectionEdit;
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy, resolve_port_policy};

use super::{GraphRuntimeError, GraphVersion, RuntimeShared, RuntimeState};

const HARD_CONNECTION_ADMISSION_WORK_LIMIT: u64 = 65_536;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ConnectionAdmissionWork {
    pub(crate) base_rules_examined: u64,
    pub(crate) incident_buckets_examined: u64,
    pub(crate) incident_entries_examined: u64,
    pub(crate) cycle_nodes_examined: u64,
    pub(crate) cycle_ports_examined: u64,
    pub(crate) cycle_buckets_examined: u64,
    pub(crate) cycle_edges_examined: u64,
}

impl ConnectionAdmissionWork {
    pub(crate) const fn total(self) -> u64 {
        self.base_rules_examined
            .saturating_add(self.incident_buckets_examined)
            .saturating_add(self.incident_entries_examined)
            .saturating_add(self.cycle_nodes_examined)
            .saturating_add(self.cycle_ports_examined)
            .saturating_add(self.cycle_buckets_examined)
            .saturating_add(self.cycle_edges_examined)
    }
}

#[derive(Debug)]
pub(crate) struct ConnectionAdmissionCursor {
    edit: ConnectionEdit,
    phase: AdmissionPhase,
    resolved: Option<ResolvedConnection>,
    removed_edges: BTreeSet<EdgeId>,
    duplicate_connect_noop: bool,
    requires_command_planning: bool,
    total_work: u64,
}

impl ConnectionAdmissionCursor {
    pub(crate) fn new(edit: ConnectionEdit) -> Self {
        Self {
            edit,
            phase: AdmissionPhase::BaseRules,
            resolved: None,
            removed_edges: BTreeSet::new(),
            duplicate_connect_noop: false,
            requires_command_planning: false,
            total_work: 0,
        }
    }
}

#[derive(Debug)]
enum AdmissionPhase {
    BaseRules,
    Duplicate(IncidentScan),
    SourceCapacity(IncidentScan),
    TargetCapacity(IncidentScan),
    StoredTypes,
    Cycle(CycleScan),
    Finish,
}

#[derive(Clone, Copy)]
enum AdmissionPhaseKind {
    BaseRules,
    Duplicate,
    SourceCapacity,
    TargetCapacity,
    StoredTypes,
    Cycle,
    Finish,
}

impl AdmissionPhase {
    fn kind(&self) -> AdmissionPhaseKind {
        match self {
            Self::BaseRules => AdmissionPhaseKind::BaseRules,
            Self::Duplicate(_) => AdmissionPhaseKind::Duplicate,
            Self::SourceCapacity(_) => AdmissionPhaseKind::SourceCapacity,
            Self::TargetCapacity(_) => AdmissionPhaseKind::TargetCapacity,
            Self::StoredTypes => AdmissionPhaseKind::StoredTypes,
            Self::Cycle(_) => AdmissionPhaseKind::Cycle,
            Self::Finish => AdmissionPhaseKind::Finish,
        }
    }
}

#[derive(Debug)]
struct IncidentScan {
    port: PortId,
    last_edge: Option<EdgeId>,
    bucket_examined: bool,
}

impl IncidentScan {
    fn new(port: PortId) -> Self {
        Self {
            port,
            last_edge: None,
            bucket_examined: false,
        }
    }
}

#[derive(Debug)]
struct CycleScan {
    source: NodeId,
    edge_kind: EdgeKind,
    skipped_edge: EdgeId,
    pending_nodes: Vec<NodeId>,
    visited_nodes: BTreeSet<NodeId>,
    visited_edges: BTreeSet<EdgeId>,
    current_node: Option<NodeId>,
    port_index: usize,
    port_examined: bool,
    bucket_examined: bool,
    last_edge: Option<EdgeId>,
}

impl CycleScan {
    fn new(source: NodeId, target: NodeId, edge_kind: EdgeKind, skipped_edge: EdgeId) -> Self {
        Self {
            source,
            edge_kind,
            skipped_edge,
            pending_nodes: vec![target],
            visited_nodes: BTreeSet::new(),
            visited_edges: BTreeSet::new(),
            current_node: None,
            port_index: 0,
            port_examined: false,
            bucket_examined: false,
            last_edge: None,
        }
    }

    fn finish_port(&mut self) {
        self.port_index = self.port_index.saturating_add(1);
        self.port_examined = false;
        self.bucket_examined = false;
        self.last_edge = None;
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedConnection {
    edge: EdgeId,
    from: PortId,
    to: PortId,
    edge_kind: EdgeKind,
    from_capacity: PortCapacity,
    to_capacity: PortCapacity,
    reconnect_edge: Option<EdgeId>,
    reconnect_noop: bool,
}

#[derive(Debug)]
pub(crate) enum ConnectionAdmissionOutcome {
    Stale,
    Incomplete {
        work: ConnectionAdmissionWork,
    },
    BudgetExhausted {
        work: ConnectionAdmissionWork,
    },
    Invalid {
        version: GraphVersion,
        diagnostics: Vec<Diagnostic>,
        work: ConnectionAdmissionWork,
    },
    Accepted {
        version: GraphVersion,
        deferred: bool,
        work: ConnectionAdmissionWork,
    },
}

impl RuntimeShared {
    pub(crate) fn advance_connection_admission(
        &self,
        cursor: &mut ConnectionAdmissionCursor,
        expected_version: GraphVersion,
        work_budget: usize,
    ) -> Result<ConnectionAdmissionOutcome, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let version = GraphVersion::new(state.provenance, state.revision);
        if version != expected_version {
            return Ok(ConnectionAdmissionOutcome::Stale);
        }

        let mut page_work = ConnectionAdmissionWork::default();
        let mut remaining = u64::try_from(work_budget).unwrap_or(u64::MAX);
        loop {
            if matches!(cursor.phase, AdmissionPhase::Finish) {
                return Ok(ConnectionAdmissionOutcome::Accepted {
                    version,
                    deferred: state.profile.is_some() || cursor.requires_command_planning,
                    work: page_work,
                });
            }
            if cursor.total_work >= HARD_CONNECTION_ADMISSION_WORK_LIMIT {
                return Ok(ConnectionAdmissionOutcome::BudgetExhausted { work: page_work });
            }

            match advance_without_or_with_work(cursor, &state, &mut page_work, &mut remaining) {
                AdmissionStep::Advanced => {}
                AdmissionStep::NeedsBudget => {
                    if cursor.total_work >= HARD_CONNECTION_ADMISSION_WORK_LIMIT {
                        return Ok(ConnectionAdmissionOutcome::BudgetExhausted { work: page_work });
                    }
                    return Ok(ConnectionAdmissionOutcome::Incomplete { work: page_work });
                }
                AdmissionStep::Invalid(diagnostics) => {
                    return Ok(ConnectionAdmissionOutcome::Invalid {
                        version,
                        diagnostics,
                        work: page_work,
                    });
                }
                AdmissionStep::Cycle => {
                    return Ok(ConnectionAdmissionOutcome::Invalid {
                        version,
                        diagnostics: vec![cycle_diagnostic(cursor.resolved_edge())],
                        work: page_work,
                    });
                }
            }
        }
    }
}

impl ConnectionAdmissionCursor {
    fn resolved(&self) -> ResolvedConnection {
        self.resolved
            .expect("admission phases after base rules retain resolved endpoints")
    }

    fn resolved_edge(&self) -> EdgeId {
        self.resolved().edge
    }

    fn spend(
        &mut self,
        remaining: &mut u64,
        page_work: &mut ConnectionAdmissionWork,
        record: impl FnOnce(&mut ConnectionAdmissionWork),
    ) -> bool {
        if *remaining == 0 || self.total_work >= HARD_CONNECTION_ADMISSION_WORK_LIMIT {
            return false;
        }
        *remaining -= 1;
        self.total_work = self.total_work.saturating_add(1);
        record(page_work);
        true
    }
}

enum AdmissionStep {
    Advanced,
    NeedsBudget,
    Invalid(Vec<Diagnostic>),
    Cycle,
}

fn advance_without_or_with_work(
    cursor: &mut ConnectionAdmissionCursor,
    state: &RuntimeState,
    page_work: &mut ConnectionAdmissionWork,
    remaining: &mut u64,
) -> AdmissionStep {
    match cursor.phase.kind() {
        AdmissionPhaseKind::BaseRules => {
            if !cursor.spend(remaining, page_work, |work| {
                work.base_rules_examined = work.base_rules_examined.saturating_add(1);
            }) {
                return AdmissionStep::NeedsBudget;
            }
            match resolve_base_connection(&state.graph, cursor.edit, &state.policy) {
                Ok(resolved) => {
                    cursor.resolved = Some(resolved);
                    cursor.phase = if resolved.reconnect_noop {
                        AdmissionPhase::StoredTypes
                    } else {
                        AdmissionPhase::Duplicate(IncidentScan::new(resolved.from))
                    };
                    AdmissionStep::Advanced
                }
                Err(diagnostics) => AdmissionStep::Invalid(diagnostics),
            }
        }
        AdmissionPhaseKind::Duplicate => {
            advance_duplicate_scan(cursor, state, page_work, remaining)
        }
        AdmissionPhaseKind::SourceCapacity => {
            advance_capacity_scan(cursor, state, page_work, remaining, true)
        }
        AdmissionPhaseKind::TargetCapacity => {
            advance_capacity_scan(cursor, state, page_work, remaining, false)
        }
        AdmissionPhaseKind::StoredTypes => {
            if !cursor.spend(remaining, page_work, |work| {
                work.base_rules_examined = work.base_rules_examined.saturating_add(1);
            }) {
                return AdmissionStep::NeedsBudget;
            }
            let resolved = cursor.resolved();
            if state.profile.is_some() {
                cursor.requires_command_planning = true;
            } else {
                match stored_type_admission(&state.graph, resolved) {
                    StoredTypeAdmission::Compatible => {}
                    StoredTypeAdmission::Deferred => cursor.requires_command_planning = true,
                    StoredTypeAdmission::Incompatible(diagnostic) => {
                        return AdmissionStep::Invalid(vec![diagnostic]);
                    }
                }
            }
            if cursor.duplicate_connect_noop || state.policy.cycles_allowed(resolved.edge_kind) {
                cursor.phase = AdmissionPhase::Finish;
                return AdmissionStep::Advanced;
            }
            let Some(source) = state.graph.port(&resolved.from).map(|port| port.node) else {
                return AdmissionStep::Invalid(vec![missing_port_diagnostic(resolved.from)]);
            };
            let Some(target) = state.graph.port(&resolved.to).map(|port| port.node) else {
                return AdmissionStep::Invalid(vec![missing_port_diagnostic(resolved.to)]);
            };
            cursor.phase = AdmissionPhase::Cycle(CycleScan::new(
                source,
                target,
                resolved.edge_kind,
                resolved.edge,
            ));
            AdmissionStep::Advanced
        }
        AdmissionPhaseKind::Cycle => advance_cycle_scan(cursor, state, page_work, remaining),
        AdmissionPhaseKind::Finish => AdmissionStep::Advanced,
    }
}

fn advance_duplicate_scan(
    cursor: &mut ConnectionAdmissionCursor,
    state: &RuntimeState,
    page_work: &mut ConnectionAdmissionWork,
    remaining: &mut u64,
) -> AdmissionStep {
    let resolved = cursor.resolved();
    let AdmissionPhase::Duplicate(scan) = &cursor.phase else {
        unreachable!("duplicate scan is only called for its matching phase");
    };
    if !scan.bucket_examined {
        if !spend_incident_bucket(cursor, remaining, page_work) {
            return AdmissionStep::NeedsBudget;
        }
        let AdmissionPhase::Duplicate(scan) = &mut cursor.phase else {
            unreachable!("phase remains duplicate while counting its bucket");
        };
        scan.bucket_examined = true;
        return AdmissionStep::Advanced;
    }
    let next = state.index.next_incident_edge(scan.port, scan.last_edge);
    let Some(edge_id) = next else {
        cursor.phase = first_capacity_phase(resolved);
        return AdmissionStep::Advanced;
    };
    if !spend_incident_entry(cursor, remaining, page_work) {
        return AdmissionStep::NeedsBudget;
    }
    let AdmissionPhase::Duplicate(scan) = &mut cursor.phase else {
        unreachable!("phase remains duplicate while advancing its entry");
    };
    scan.last_edge = Some(edge_id);
    let Some(edge) = state.graph.edge(&edge_id) else {
        return AdmissionStep::Advanced;
    };
    if Some(edge_id) == resolved.reconnect_edge
        || edge.kind != resolved.edge_kind
        || edge.from != resolved.from
        || edge.to != resolved.to
    {
        return AdmissionStep::Advanced;
    }
    if resolved.reconnect_edge.is_some() {
        return AdmissionStep::Invalid(vec![duplicate_diagnostic(resolved.to)]);
    }
    cursor.duplicate_connect_noop = true;
    cursor.phase = AdmissionPhase::StoredTypes;
    AdmissionStep::Advanced
}

fn first_capacity_phase(resolved: ResolvedConnection) -> AdmissionPhase {
    if resolved.from_capacity == PortCapacity::Single {
        AdmissionPhase::SourceCapacity(IncidentScan::new(resolved.from))
    } else if resolved.to_capacity == PortCapacity::Single {
        AdmissionPhase::TargetCapacity(IncidentScan::new(resolved.to))
    } else {
        AdmissionPhase::StoredTypes
    }
}

fn advance_capacity_scan(
    cursor: &mut ConnectionAdmissionCursor,
    state: &RuntimeState,
    page_work: &mut ConnectionAdmissionWork,
    remaining: &mut u64,
    source: bool,
) -> AdmissionStep {
    let resolved = cursor.resolved();
    let scan = match &cursor.phase {
        AdmissionPhase::SourceCapacity(scan) | AdmissionPhase::TargetCapacity(scan) => scan,
        _ => unreachable!("capacity scan is only called for a capacity phase"),
    };
    if !scan.bucket_examined {
        if !spend_incident_bucket(cursor, remaining, page_work) {
            return AdmissionStep::NeedsBudget;
        }
        let scan = match &mut cursor.phase {
            AdmissionPhase::SourceCapacity(scan) | AdmissionPhase::TargetCapacity(scan) => scan,
            _ => unreachable!("phase remains capacity while counting its bucket"),
        };
        scan.bucket_examined = true;
        return AdmissionStep::Advanced;
    }
    let next = state.index.next_incident_edge(scan.port, scan.last_edge);
    let Some(edge_id) = next else {
        cursor.phase = if source && resolved.to_capacity == PortCapacity::Single {
            AdmissionPhase::TargetCapacity(IncidentScan::new(resolved.to))
        } else {
            AdmissionPhase::StoredTypes
        };
        return AdmissionStep::Advanced;
    };
    if !spend_incident_entry(cursor, remaining, page_work) {
        return AdmissionStep::NeedsBudget;
    }
    let scan = match &mut cursor.phase {
        AdmissionPhase::SourceCapacity(scan) | AdmissionPhase::TargetCapacity(scan) => scan,
        _ => unreachable!("phase remains capacity while advancing its entry"),
    };
    scan.last_edge = Some(edge_id);
    if Some(edge_id) == resolved.reconnect_edge {
        return AdmissionStep::Advanced;
    }
    let Some(edge) = state.graph.edge(&edge_id) else {
        return AdmissionStep::Invalid(vec![missing_edge_diagnostic(edge_id)]);
    };
    let occupies_endpoint = edge.kind == resolved.edge_kind
        && if source {
            edge.from == resolved.from
        } else {
            edge.to == resolved.to
        };
    if !occupies_endpoint || !cursor.removed_edges.insert(edge_id) {
        return AdmissionStep::Advanced;
    }
    if !resolve_edge_policy(edge, &state.policy).can_delete() {
        return AdmissionStep::Invalid(vec![edge_not_deletable_diagnostic(edge_id)]);
    }
    AdmissionStep::Advanced
}

fn advance_cycle_scan(
    cursor: &mut ConnectionAdmissionCursor,
    state: &RuntimeState,
    page_work: &mut ConnectionAdmissionWork,
    remaining: &mut u64,
) -> AdmissionStep {
    let (current_node, pending_node) = match &cursor.phase {
        AdmissionPhase::Cycle(scan) => (scan.current_node, scan.pending_nodes.last().copied()),
        _ => unreachable!("cycle scan is only called for its matching phase"),
    };
    if current_node.is_none() {
        let Some(node) = pending_node else {
            cursor.phase = AdmissionPhase::Finish;
            return AdmissionStep::Advanced;
        };
        if !cursor.spend(remaining, page_work, |work| {
            work.cycle_nodes_examined = work.cycle_nodes_examined.saturating_add(1);
        }) {
            return AdmissionStep::NeedsBudget;
        }
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while starting a node");
        };
        let popped = scan.pending_nodes.pop();
        debug_assert_eq!(popped, Some(node));
        if node == scan.source {
            return AdmissionStep::Cycle;
        }
        if scan.visited_nodes.insert(node) && state.graph.node(&node).is_some() {
            scan.current_node = Some(node);
            scan.port_index = 0;
        }
        return AdmissionStep::Advanced;
    }

    let node_id = current_node.expect("cycle node checked above");
    let Some(node) = state.graph.node(&node_id) else {
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while skipping a missing node");
        };
        scan.current_node = None;
        return AdmissionStep::Advanced;
    };
    let (port_index, port_examined, bucket_examined, last_edge) = match &cursor.phase {
        AdmissionPhase::Cycle(scan) => (
            scan.port_index,
            scan.port_examined,
            scan.bucket_examined,
            scan.last_edge,
        ),
        _ => unreachable!("phase remains cycle while reading its position"),
    };
    if port_index >= node.ports.len() {
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while finishing a node");
        };
        scan.current_node = None;
        return AdmissionStep::Advanced;
    }
    let port = node.ports[port_index];
    if !port_examined {
        if !cursor.spend(remaining, page_work, |work| {
            work.cycle_ports_examined = work.cycle_ports_examined.saturating_add(1);
        }) {
            return AdmissionStep::NeedsBudget;
        }
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while counting a port");
        };
        scan.port_examined = true;
        return AdmissionStep::Advanced;
    }
    if !bucket_examined {
        if !cursor.spend(remaining, page_work, |work| {
            work.cycle_buckets_examined = work.cycle_buckets_examined.saturating_add(1);
        }) {
            return AdmissionStep::NeedsBudget;
        }
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while counting an incident bucket");
        };
        scan.bucket_examined = true;
        return AdmissionStep::Advanced;
    }
    let next = state.index.next_incident_edge(port, last_edge);
    let Some(edge_id) = next else {
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while finishing a port");
        };
        scan.finish_port();
        return AdmissionStep::Advanced;
    };
    if !cursor.spend(remaining, page_work, |work| {
        work.cycle_edges_examined = work.cycle_edges_examined.saturating_add(1);
    }) {
        return AdmissionStep::NeedsBudget;
    }
    let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
        unreachable!("phase remains cycle while advancing an edge");
    };
    scan.last_edge = Some(edge_id);
    let skipped_edge = scan.skipped_edge;
    if edge_id == skipped_edge || cursor.removed_edges.contains(&edge_id) {
        return AdmissionStep::Advanced;
    }
    let first_visit = match &mut cursor.phase {
        AdmissionPhase::Cycle(scan) => scan.visited_edges.insert(edge_id),
        _ => unreachable!("phase remains cycle while tracking an edge"),
    };
    if !first_visit {
        return AdmissionStep::Advanced;
    }
    let Some(edge) = state.graph.edge(&edge_id) else {
        return AdmissionStep::Advanced;
    };
    let edge_kind = match &cursor.phase {
        AdmissionPhase::Cycle(scan) => scan.edge_kind,
        _ => unreachable!("phase remains cycle while checking edge semantics"),
    };
    if edge.kind != edge_kind || edge.from != port {
        return AdmissionStep::Advanced;
    }
    if let Some(next_node) = state.graph.port(&edge.to).map(|port| port.node) {
        let AdmissionPhase::Cycle(scan) = &mut cursor.phase else {
            unreachable!("phase remains cycle while queuing a node");
        };
        scan.pending_nodes.push(next_node);
    }
    AdmissionStep::Advanced
}

fn spend_incident_bucket(
    cursor: &mut ConnectionAdmissionCursor,
    remaining: &mut u64,
    page_work: &mut ConnectionAdmissionWork,
) -> bool {
    cursor.spend(remaining, page_work, |work| {
        work.incident_buckets_examined = work.incident_buckets_examined.saturating_add(1);
    })
}

fn spend_incident_entry(
    cursor: &mut ConnectionAdmissionCursor,
    remaining: &mut u64,
    page_work: &mut ConnectionAdmissionWork,
) -> bool {
    cursor.spend(remaining, page_work, |work| {
        work.incident_entries_examined = work.incident_entries_examined.saturating_add(1);
    })
}

fn resolve_base_connection(
    graph: &Graph,
    edit: ConnectionEdit,
    policy: &GraphRuntimePolicy,
) -> Result<ResolvedConnection, Vec<Diagnostic>> {
    match edit {
        ConnectionEdit::Connect {
            edge,
            from,
            to,
            mode,
        } => {
            if graph.edge(&edge).is_some() {
                return Err(vec![edge_already_exists_diagnostic(edge)]);
            }
            let (from, to) = resolve_connect_order(graph, from, to, mode)?;
            resolve_endpoints(graph, edge, from, to, None, false, policy)
        }
        ConnectionEdit::Reconnect {
            edge,
            endpoint,
            new_port,
            mode,
        } => {
            let Some(current) = graph.edge(&edge) else {
                return Err(vec![missing_edge_diagnostic(edge)]);
            };
            let edge_policy = resolve_edge_policy(current, policy);
            let reconnectable = match endpoint {
                EdgeEndpoint::From => edge_policy.can_reconnect_source(),
                EdgeEndpoint::To => edge_policy.can_reconnect_target(),
            };
            if !reconnectable {
                return Err(vec![edge_not_reconnectable_diagnostic(edge, endpoint)]);
            }
            let (from, to) = match endpoint {
                EdgeEndpoint::From => (new_port, current.to),
                EdgeEndpoint::To => (current.from, new_port),
            };
            if from == to {
                return Err(vec![self_connection_diagnostic(from)]);
            }
            let reconnect_noop = from == current.from && to == current.to;
            let resolved =
                resolve_endpoints(graph, edge, from, to, Some(edge), reconnect_noop, policy)?;
            if reconnect_noop {
                return Ok(resolved);
            }
            if mode == NodeGraphConnectionMode::Strict
                && !matches!(
                    (
                        graph.port(&from).map(|port| port.dir),
                        graph.port(&to).map(|port| port.dir),
                    ),
                    (Some(PortDirection::Out), Some(PortDirection::In))
                )
            {
                return Err(vec![reconnect_direction_diagnostic(to)]);
            }
            validate_endpoint_policy(graph, from, to, policy)?;
            if current.kind != resolved.edge_kind {
                return Err(vec![edge_kind_diagnostic(
                    current.kind,
                    resolved.edge_kind,
                    to,
                )]);
            }
            Ok(resolved)
        }
        ConnectionEdit::Disconnect { edge } => Err(vec![Diagnostic::error(
            "connect.probe_disconnect",
            DiagnosticTarget::Edge { id: edge },
            "disconnect commands are not connection targets",
        )]),
    }
}

fn resolve_connect_order(
    graph: &Graph,
    first: PortId,
    second: PortId,
    mode: NodeGraphConnectionMode,
) -> Result<(PortId, PortId), Vec<Diagnostic>> {
    if first == second {
        return Err(vec![self_connection_diagnostic(first)]);
    }
    let Some(first_port) = graph.port(&first) else {
        return Err(vec![missing_port_diagnostic(first)]);
    };
    let Some(second_port) = graph.port(&second) else {
        return Err(vec![missing_port_diagnostic(second)]);
    };
    match (mode, first_port.dir, second_port.dir) {
        (_, PortDirection::Out, PortDirection::In) => Ok((first, second)),
        (_, PortDirection::In, PortDirection::Out) => Ok((second, first)),
        (NodeGraphConnectionMode::Loose, PortDirection::Out, PortDirection::Out) => {
            Ok((first, second))
        }
        (NodeGraphConnectionMode::Loose, PortDirection::In, PortDirection::In) => {
            Ok((second, first))
        }
        (NodeGraphConnectionMode::Strict, _, _) => Err(vec![direction_diagnostic(second)]),
    }
}

fn resolve_endpoints(
    graph: &Graph,
    edge: EdgeId,
    from: PortId,
    to: PortId,
    reconnect_edge: Option<EdgeId>,
    reconnect_noop: bool,
    policy: &GraphRuntimePolicy,
) -> Result<ResolvedConnection, Vec<Diagnostic>> {
    if from == to {
        return Err(vec![self_connection_diagnostic(from)]);
    }
    let Some(from_port) = graph.port(&from) else {
        return Err(vec![missing_port_diagnostic(from)]);
    };
    let Some(to_port) = graph.port(&to) else {
        return Err(vec![missing_port_diagnostic(to)]);
    };
    if from_port.kind != to_port.kind {
        return Err(vec![port_kind_diagnostic(from_port.kind, to_port.kind, to)]);
    }
    if reconnect_edge.is_none() {
        validate_endpoint_policy(graph, from, to, policy)?;
    }
    Ok(ResolvedConnection {
        edge,
        from,
        to,
        edge_kind: from_port.kind.edge_kind(),
        from_capacity: from_port.capacity,
        to_capacity: to_port.capacity,
        reconnect_edge,
        reconnect_noop,
    })
}

fn validate_endpoint_policy(
    graph: &Graph,
    from: PortId,
    to: PortId,
    policy: &GraphRuntimePolicy,
) -> Result<(), Vec<Diagnostic>> {
    let from_port = graph
        .port(&from)
        .ok_or_else(|| vec![missing_port_diagnostic(from)])?;
    let to_port = graph
        .port(&to)
        .ok_or_else(|| vec![missing_port_diagnostic(to)])?;
    let Some(from_owner) = graph.node(&from_port.node) else {
        return Err(vec![missing_owner_diagnostic(from_port.node)]);
    };
    let Some(to_owner) = graph.node(&to_port.node) else {
        return Err(vec![missing_owner_diagnostic(to_port.node)]);
    };
    if !resolve_port_policy(from_owner, from_port, policy).can_start_connection() {
        return Err(vec![port_connectability_diagnostic(from, true)]);
    }
    if !resolve_port_policy(to_owner, to_port, policy).can_accept_connection() {
        return Err(vec![port_connectability_diagnostic(to, false)]);
    }
    Ok(())
}

enum StoredTypeAdmission {
    Compatible,
    Deferred,
    Incompatible(Diagnostic),
}

fn stored_type_admission(graph: &Graph, resolved: ResolvedConnection) -> StoredTypeAdmission {
    if resolved.edge_kind != EdgeKind::Data {
        return StoredTypeAdmission::Compatible;
    }
    let Some(from) = graph.port(&resolved.from).and_then(|port| port.ty.as_ref()) else {
        return StoredTypeAdmission::Compatible;
    };
    let Some(to) = graph.port(&resolved.to).and_then(|port| port.ty.as_ref()) else {
        return StoredTypeAdmission::Compatible;
    };
    if type_requires_structural_walk(from) || type_requires_structural_walk(to) {
        return StoredTypeAdmission::Deferred;
    }
    let TypeCompatibilityResult::Incompatible { reason } =
        DefaultTypeCompatibility.compatible(from, to)
    else {
        return StoredTypeAdmission::Compatible;
    };
    StoredTypeAdmission::Incompatible(Diagnostic::error(
        "connect.type_mismatch",
        DiagnosticTarget::Port { id: resolved.to },
        format!("type mismatch: {reason} (from={from:?} to={to:?})"),
    ))
}

fn type_requires_structural_walk(ty: &jellyflow_core::types::TypeDesc) -> bool {
    matches!(
        ty,
        jellyflow_core::types::TypeDesc::List { .. }
            | jellyflow_core::types::TypeDesc::Map { .. }
            | jellyflow_core::types::TypeDesc::Object { .. }
            | jellyflow_core::types::TypeDesc::Union { .. }
            | jellyflow_core::types::TypeDesc::Option { .. }
            | jellyflow_core::types::TypeDesc::Opaque { .. }
    )
}

fn missing_port_diagnostic(port: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.port_missing",
        DiagnosticTarget::Port { id: port },
        format!("missing port: {port:?}"),
    )
}

fn missing_owner_diagnostic(node: NodeId) -> Diagnostic {
    Diagnostic::error(
        "connect.port_owner_missing",
        DiagnosticTarget::Node { id: node },
        format!("missing port owner node: {node:?}"),
    )
}

fn missing_edge_diagnostic(edge: EdgeId) -> Diagnostic {
    Diagnostic::error(
        "connect.edge_missing",
        DiagnosticTarget::Edge { id: edge },
        format!("missing edge: {edge:?}"),
    )
}

fn edge_already_exists_diagnostic(edge: EdgeId) -> Diagnostic {
    Diagnostic::error(
        "connect.edge_already_exists",
        DiagnosticTarget::Edge { id: edge },
        format!("edge already exists: {edge:?}"),
    )
}

fn self_connection_diagnostic(port: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.self_connection",
        DiagnosticTarget::Port { id: port },
        "cannot connect a port to itself",
    )
}

fn direction_diagnostic(port: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.direction_mismatch",
        DiagnosticTarget::Port { id: port },
        "ports must have opposite directions (in/out)",
    )
}

fn reconnect_direction_diagnostic(port: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.reconnect_direction_mismatch",
        DiagnosticTarget::Port { id: port },
        "ports must be out -> in for reconnection",
    )
}

fn port_kind_diagnostic(from: PortKind, to: PortKind, target: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.port_kind_mismatch",
        DiagnosticTarget::Port { id: target },
        format!("port kinds are incompatible: from={from:?} to={to:?}"),
    )
}

fn edge_kind_diagnostic(edge: EdgeKind, expected: EdgeKind, target: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.edge_kind_mismatch",
        DiagnosticTarget::Port { id: target },
        format!("edge kind is incompatible with ports: edge={edge:?} expected={expected:?}"),
    )
}

fn port_connectability_diagnostic(port: PortId, source: bool) -> Diagnostic {
    let (key, message) = if source {
        (
            "connect.source_not_connectable",
            "source port is not connectable",
        )
    } else {
        (
            "connect.target_not_connectable",
            "target port is not connectable",
        )
    };
    Diagnostic::error(key, DiagnosticTarget::Port { id: port }, message)
}

fn duplicate_diagnostic(port: PortId) -> Diagnostic {
    Diagnostic::error(
        "connect.duplicate",
        DiagnosticTarget::Port { id: port },
        "duplicate connection already exists",
    )
}

fn edge_not_reconnectable_diagnostic(edge: EdgeId, endpoint: EdgeEndpoint) -> Diagnostic {
    let message = match endpoint {
        EdgeEndpoint::From => "edge source endpoint is not reconnectable",
        EdgeEndpoint::To => "edge target endpoint is not reconnectable",
    };
    Diagnostic::error(
        "connect.edge_not_reconnectable",
        DiagnosticTarget::Edge { id: edge },
        message,
    )
}

fn edge_not_deletable_diagnostic(edge: EdgeId) -> Diagnostic {
    Diagnostic::error(
        "delete.edge_not_deletable",
        DiagnosticTarget::Edge { id: edge },
        format!("edge is not deletable: {edge:?}"),
    )
}

fn cycle_diagnostic(edge: EdgeId) -> Diagnostic {
    Diagnostic::error(
        "connect.cycle",
        DiagnosticTarget::Edge { id: edge },
        "connection would create a disallowed cycle",
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use jellyflow_core::core::{
        CanvasPoint, Edge, GraphBuilder, GraphId, Node, NodeKindKey, Port, PortKey,
    };
    use jellyflow_core::types::{TypeCompatibility, TypeDesc};

    use crate::profile::GraphProfile;
    use crate::runtime::commands::GraphCommand;
    use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimePolicy};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Verdict {
        Invalid,
        Accepted { deferred: bool },
        BudgetExhausted,
    }

    fn add_work(total: &mut ConnectionAdmissionWork, page: ConnectionAdmissionWork) {
        total.base_rules_examined = total
            .base_rules_examined
            .saturating_add(page.base_rules_examined);
        total.incident_buckets_examined = total
            .incident_buckets_examined
            .saturating_add(page.incident_buckets_examined);
        total.incident_entries_examined = total
            .incident_entries_examined
            .saturating_add(page.incident_entries_examined);
        total.cycle_nodes_examined = total
            .cycle_nodes_examined
            .saturating_add(page.cycle_nodes_examined);
        total.cycle_ports_examined = total
            .cycle_ports_examined
            .saturating_add(page.cycle_ports_examined);
        total.cycle_buckets_examined = total
            .cycle_buckets_examined
            .saturating_add(page.cycle_buckets_examined);
        total.cycle_edges_examined = total
            .cycle_edges_examined
            .saturating_add(page.cycle_edges_examined);
    }

    fn drain(
        runtime: &GraphRuntime,
        edit: ConnectionEdit,
        budget: usize,
    ) -> (Verdict, ConnectionAdmissionWork, usize) {
        assert!(budget > 0, "terminal drain requires progress");
        let version = runtime.snapshot().expect("snapshot").version();
        let mut cursor = ConnectionAdmissionCursor::new(edit);
        let mut total = ConnectionAdmissionWork::default();
        for page in 1..100_000 {
            let outcome = runtime
                .shared
                .advance_connection_admission(&mut cursor, version, budget)
                .expect("admission page");
            match outcome {
                ConnectionAdmissionOutcome::Stale => panic!("stable fixture became stale"),
                ConnectionAdmissionOutcome::Incomplete { work } => {
                    assert!(work.total() <= budget as u64);
                    add_work(&mut total, work);
                }
                ConnectionAdmissionOutcome::BudgetExhausted { work } => {
                    assert!(work.total() <= budget as u64);
                    add_work(&mut total, work);
                    return (Verdict::BudgetExhausted, total, page);
                }
                ConnectionAdmissionOutcome::Invalid { work, .. } => {
                    assert!(work.total() <= budget as u64);
                    add_work(&mut total, work);
                    return (Verdict::Invalid, total, page);
                }
                ConnectionAdmissionOutcome::Accepted { deferred, work, .. } => {
                    assert!(work.total() <= budget as u64);
                    add_work(&mut total, work);
                    return (Verdict::Accepted { deferred }, total, page);
                }
            }
        }
        panic!("admission did not terminate within the logical hard bound")
    }

    #[test]
    fn zero_budget_does_not_advance_base_admission() {
        let (runtime, source, target) = two_port_runtime(None, GraphRuntimePolicy::default());
        let version = runtime.snapshot().unwrap().version();
        let mut cursor = ConnectionAdmissionCursor::new(ConnectionEdit::connect(
            EdgeId::from_u128(90),
            source,
            target,
            NodeGraphConnectionMode::Strict,
        ));

        let ConnectionAdmissionOutcome::Incomplete { work } = runtime
            .shared
            .advance_connection_admission(&mut cursor, version, 0)
            .unwrap()
        else {
            panic!("zero budget must retain the continuation");
        };
        assert_eq!(work.total(), 0);
    }

    #[test]
    fn high_fanout_duplicate_admission_never_exceeds_one_unit_per_page() {
        const FANOUT: usize = 256;
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(2);
        let candidate_node = NodeId::from_u128(3);
        let candidate_port = PortId::from_u128(4);
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        builder.insert_node(source_node, node_with_ports("source", vec![source_port]));
        builder.insert_port(
            source_port,
            port(
                source_node,
                "source",
                PortDirection::Out,
                PortCapacity::Multi,
                TypeDesc::Float,
            ),
        );
        builder.insert_node(
            candidate_node,
            node_with_ports("candidate", vec![candidate_port]),
        );
        builder.insert_port(
            candidate_port,
            port(
                candidate_node,
                "candidate",
                PortDirection::In,
                PortCapacity::Multi,
                TypeDesc::Float,
            ),
        );
        for offset in 0..FANOUT {
            let node_id = NodeId::from_u128(100 + offset as u128);
            let port_id = PortId::from_u128(1_000 + offset as u128);
            builder.insert_node(node_id, node_with_ports("fanout", vec![port_id]));
            builder.insert_port(
                port_id,
                port(
                    node_id,
                    "fanout",
                    PortDirection::In,
                    PortCapacity::Multi,
                    TypeDesc::Float,
                ),
            );
            builder.insert_edge(
                EdgeId::from_u128(10_000 + offset as u128),
                Edge::new(EdgeKind::Data, source_port, port_id),
            );
        }
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("fanout graph"))
            .expect("runtime");
        let edit = ConnectionEdit::connect(
            EdgeId::from_u128(99_999),
            source_port,
            candidate_port,
            NodeGraphConnectionMode::Strict,
        );

        let (verdict, work, pages) = drain(&runtime, edit, 1);

        assert_eq!(verdict, Verdict::Accepted { deferred: false });
        assert!(pages > FANOUT);
        assert_eq!(work.incident_entries_examined, FANOUT as u64);
    }

    #[test]
    fn single_capacity_replacement_obeys_authoritative_edge_deletion_policy() {
        for (policy, expected) in [
            (
                GraphRuntimePolicy::default(),
                Verdict::Accepted { deferred: false },
            ),
            (
                GraphRuntimePolicy::default().with_edge_deletion(false),
                Verdict::Invalid,
            ),
        ] {
            let (runtime, source, target) = single_capacity_runtime(policy);
            let (verdict, work, _) = drain(
                &runtime,
                ConnectionEdit::connect(
                    EdgeId::from_u128(99_998),
                    source,
                    target,
                    NodeGraphConnectionMode::Strict,
                ),
                1,
            );

            assert_eq!(verdict, expected);
            assert_eq!(work.incident_entries_examined, 1);
        }
    }

    #[test]
    fn long_cycle_scan_is_split_invariant_and_bounded() {
        let (runtime, source, target) = chain_runtime(96);
        let edit = ConnectionEdit::connect(
            EdgeId::from_u128(500_000),
            source,
            target,
            NodeGraphConnectionMode::Strict,
        );

        let one = drain(&runtime, edit, 1);
        let many = drain(&runtime, edit, 23);

        assert_eq!(one.0, Verdict::Invalid);
        assert_eq!(one.0, many.0);
        assert_eq!(one.1, many.1);
        assert!(one.2 > many.2);
        assert!(one.1.cycle_nodes_examined >= 96);
        assert!(one.1.cycle_edges_examined >= 95);
    }

    #[test]
    fn shader_cycle_policy_accepts_acyclic_data_and_rejects_cycle_closure() {
        let policy = GraphRuntimePolicy::default().with_cycles(EdgeKind::Data, false);
        let (acyclic, source, target) = two_port_runtime(None, policy);
        let accepted = drain(
            &acyclic,
            ConnectionEdit::connect(
                EdgeId::from_u128(90),
                source,
                target,
                NodeGraphConnectionMode::Strict,
            ),
            3,
        );
        assert_eq!(accepted.0, Verdict::Accepted { deferred: false });

        let (cyclic, source, target) = chain_runtime(4);
        let rejected = drain(
            &cyclic,
            ConnectionEdit::connect(
                EdgeId::from_u128(91),
                source,
                target,
                NodeGraphConnectionMode::Strict,
            ),
            3,
        );
        assert_eq!(rejected.0, Verdict::Invalid);
    }

    #[test]
    fn profile_hooks_are_deferred_until_authoritative_command_planning() {
        let calls = Rc::new(Cell::new(0));
        let profile = CountingRejectProfile {
            calls: Rc::clone(&calls),
        };
        let (runtime, source, target) =
            two_port_runtime(Some(Box::new(profile)), GraphRuntimePolicy::default());
        calls.set(0);
        let edge = EdgeId::from_u128(90);
        let edit = ConnectionEdit::connect(edge, source, target, NodeGraphConnectionMode::Strict);

        let (verdict, _, _) = drain(&runtime, edit, 1);
        assert_eq!(verdict, Verdict::Accepted { deferred: true });
        assert_eq!(
            calls.get(),
            0,
            "probe admission must not invoke profile hooks"
        );

        let version = runtime.snapshot().unwrap().version();
        let completion = runtime.execute(GraphCommand::connect(
            version,
            edge,
            source,
            target,
            NodeGraphConnectionMode::Strict,
        ));
        assert!(completion.into_result().unwrap().rejection().is_some());
        assert!(calls.get() > 0, "command planning still owns profile hooks");
    }

    #[test]
    fn custom_profile_does_not_inherit_default_stored_type_rejection_during_probe() {
        let calls = Rc::new(Cell::new(0));
        let (runtime, source, target) = two_port_runtime_with_types(
            Some(Box::new(CountingAcceptProfile {
                calls: Rc::clone(&calls),
            })),
            GraphRuntimePolicy::default(),
            TypeDesc::Float,
            TypeDesc::String,
        );
        calls.set(0);
        let edge = EdgeId::from_u128(92);
        let edit = ConnectionEdit::connect(edge, source, target, NodeGraphConnectionMode::Strict);

        assert_eq!(
            drain(&runtime, edit, 1).0,
            Verdict::Accepted { deferred: true }
        );
        assert_eq!(calls.get(), 0);

        let completion = runtime.execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            edge,
            source,
            target,
            NodeGraphConnectionMode::Strict,
        ));
        assert!(completion.into_result().unwrap().applied().is_some());
        assert!(calls.get() > 0);
    }

    #[test]
    fn structural_default_types_defer_without_recursive_probe_work() {
        let wide_union = TypeDesc::Union {
            types: vec![TypeDesc::String; 8_192],
        };
        let (runtime, source, target) = two_port_runtime_with_types(
            None,
            GraphRuntimePolicy::default(),
            TypeDesc::Int,
            wide_union,
        );

        let (verdict, work, _) = drain(
            &runtime,
            ConnectionEdit::connect(
                EdgeId::from_u128(93),
                source,
                target,
                NodeGraphConnectionMode::Strict,
            ),
            1,
        );

        assert_eq!(verdict, Verdict::Accepted { deferred: true });
        assert_eq!(work.base_rules_examined, 2);
    }

    #[test]
    fn reconnect_noop_matches_authoritative_planner_before_port_policy_checks() {
        let (base, source, target) = two_port_runtime(None, GraphRuntimePolicy::default());
        let mut builder =
            GraphBuilder::from_detached_graph(base.snapshot().unwrap().graph().clone());
        builder.port_mut(&source).expect("source").connectable = Some(false);
        builder.port_mut(&target).expect("target").connectable = Some(false);
        let edge = EdgeId::from_u128(94);
        builder.insert_edge(edge, Edge::new(EdgeKind::Data, source, target));
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().expect("noop reconnect graph"))
            .expect("runtime");
        let edit = ConnectionEdit::reconnect(
            edge,
            EdgeEndpoint::To,
            target,
            NodeGraphConnectionMode::Strict,
        );

        assert!(runtime.evaluate_connection(edit).unwrap().is_accept());
        assert_eq!(
            drain(&runtime, edit, 1).0,
            Verdict::Accepted { deferred: false }
        );
    }

    fn two_port_runtime(
        profile: Option<Box<dyn GraphProfile>>,
        policy: GraphRuntimePolicy,
    ) -> (GraphRuntime, PortId, PortId) {
        two_port_runtime_with_types(profile, policy, TypeDesc::Float, TypeDesc::Float)
    }

    fn two_port_runtime_with_types(
        profile: Option<Box<dyn GraphProfile>>,
        policy: GraphRuntimePolicy,
        source_type: TypeDesc,
        target_type: TypeDesc,
    ) -> (GraphRuntime, PortId, PortId) {
        let source_node = NodeId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let source = PortId::from_u128(11);
        let target = PortId::from_u128(12);
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        builder.insert_node(source_node, node_with_ports("source", vec![source]));
        builder.insert_port(
            source,
            port(
                source_node,
                "source",
                PortDirection::Out,
                PortCapacity::Multi,
                source_type,
            ),
        );
        builder.insert_node(target_node, node_with_ports("target", vec![target]));
        builder.insert_port(
            target,
            port(
                target_node,
                "target",
                PortDirection::In,
                PortCapacity::Multi,
                target_type,
            ),
        );
        let mut runtime = GraphRuntime::builder(Default::default()).with_policy(policy);
        if let Some(profile) = profile {
            runtime = runtime.with_profile(profile);
        }
        (
            runtime
                .build(builder.build().expect("two-port graph"))
                .expect("runtime"),
            source,
            target,
        )
    }

    fn single_capacity_runtime(policy: GraphRuntimePolicy) -> (GraphRuntime, PortId, PortId) {
        let old_source_node = NodeId::from_u128(1);
        let new_source_node = NodeId::from_u128(2);
        let target_node = NodeId::from_u128(3);
        let old_source = PortId::from_u128(11);
        let new_source = PortId::from_u128(12);
        let target = PortId::from_u128(13);
        let mut builder = GraphBuilder::new(GraphId::from_u128(3));
        for (node, port_id, key) in [
            (old_source_node, old_source, "old-source"),
            (new_source_node, new_source, "new-source"),
        ] {
            builder.insert_node(node, node_with_ports(key, vec![port_id]));
            builder.insert_port(
                port_id,
                port(
                    node,
                    key,
                    PortDirection::Out,
                    PortCapacity::Multi,
                    TypeDesc::Float,
                ),
            );
        }
        builder.insert_node(target_node, node_with_ports("target", vec![target]));
        builder.insert_port(
            target,
            port(
                target_node,
                "target",
                PortDirection::In,
                PortCapacity::Single,
                TypeDesc::Float,
            ),
        );
        builder.insert_edge(
            EdgeId::from_u128(50),
            Edge::new(EdgeKind::Data, old_source, target),
        );
        (
            GraphRuntime::builder(Default::default())
                .with_policy(policy)
                .build(builder.build().expect("single-capacity graph"))
                .expect("runtime"),
            new_source,
            target,
        )
    }

    fn chain_runtime(length: usize) -> (GraphRuntime, PortId, PortId) {
        let mut builder = GraphBuilder::new(GraphId::from_u128(2));
        let mut inputs = Vec::with_capacity(length);
        let mut outputs = Vec::with_capacity(length);
        for offset in 0..length {
            let node = NodeId::from_u128(1_000 + offset as u128);
            let input = PortId::from_u128(10_000 + offset as u128 * 2);
            let output = PortId::from_u128(10_001 + offset as u128 * 2);
            builder.insert_node(node, node_with_ports("chain", vec![input, output]));
            builder.insert_port(
                input,
                port(
                    node,
                    "input",
                    PortDirection::In,
                    PortCapacity::Multi,
                    TypeDesc::Float,
                ),
            );
            builder.insert_port(
                output,
                port(
                    node,
                    "output",
                    PortDirection::Out,
                    PortCapacity::Multi,
                    TypeDesc::Float,
                ),
            );
            inputs.push(input);
            outputs.push(output);
        }
        for offset in 0..length.saturating_sub(1) {
            builder.insert_edge(
                EdgeId::from_u128(100_000 + offset as u128),
                Edge::new(EdgeKind::Data, outputs[offset], inputs[offset + 1]),
            );
        }
        let runtime = GraphRuntime::builder(Default::default())
            .with_policy(GraphRuntimePolicy::default().with_cycles(EdgeKind::Data, false))
            .build(builder.build().expect("chain graph"))
            .expect("runtime");
        (runtime, outputs[length - 1], inputs[0])
    }

    fn node_with_ports(kind: &str, ports: Vec<PortId>) -> Node {
        Node {
            kind: NodeKindKey::new(kind),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports,
            data: serde_json::Value::Null,
        }
    }

    fn port(
        node: NodeId,
        key: &str,
        dir: PortDirection,
        capacity: PortCapacity,
        ty: TypeDesc,
    ) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: Some(ty),
            data: serde_json::Value::Null,
        }
    }

    struct CountingRejectProfile {
        calls: Rc<Cell<u64>>,
    }

    impl GraphProfile for CountingRejectProfile {
        fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            self.calls.set(self.calls.get().saturating_add(1));
            graph.port(&port).and_then(|port| port.ty.clone())
        }

        fn type_compatibility(&self) -> &dyn TypeCompatibility {
            self.calls.set(self.calls.get().saturating_add(1));
            &RejectCompatibility
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            self.calls.set(self.calls.get().saturating_add(1));
            Vec::new()
        }

        fn allow_cycles(&self, _edge_kind: EdgeKind) -> bool {
            self.calls.set(self.calls.get().saturating_add(1));
            true
        }
    }

    struct RejectCompatibility;

    impl TypeCompatibility for RejectCompatibility {
        fn compatible(&self, _from: &TypeDesc, _to: &TypeDesc) -> TypeCompatibilityResult {
            TypeCompatibilityResult::Incompatible {
                reason: "profile-owned rejection".to_owned(),
            }
        }
    }

    struct CountingAcceptProfile {
        calls: Rc<Cell<u64>>,
    }

    impl GraphProfile for CountingAcceptProfile {
        fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            self.calls.set(self.calls.get().saturating_add(1));
            graph.port(&port).and_then(|port| port.ty.clone())
        }

        fn type_compatibility(&self) -> &dyn TypeCompatibility {
            self.calls.set(self.calls.get().saturating_add(1));
            &AcceptCompatibility
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            self.calls.set(self.calls.get().saturating_add(1));
            Vec::new()
        }
    }

    struct AcceptCompatibility;

    impl TypeCompatibility for AcceptCompatibility {
        fn compatible(&self, _from: &TypeDesc, _to: &TypeDesc) -> TypeCompatibilityResult {
            TypeCompatibilityResult::Compatible
        }
    }
}
