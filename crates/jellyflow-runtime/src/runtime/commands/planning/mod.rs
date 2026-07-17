use std::collections::BTreeSet;

use crate::graph_kernel::{GraphMutationError, GraphMutationFootprint, GraphOp, GraphTransaction};
use jellyflow_core::core::{EdgeId, Graph, NodeId, NodeKindKey, PortId, SymbolId};

use crate::profile::{ApplyPipelineError, GraphProfile};
use crate::rules::Diagnostic;
use crate::runtime::graph_runtime::GraphIndex;
use crate::runtime::policy::GraphRuntimePolicy;
use crate::runtime::transaction_gate::GraphTransactionGate;
use crate::schema::NodeCatalog;

use super::model::{GraphCommand, GraphCommandAction, IncidentEdgePolicy};

mod connection;
mod context;
mod profile;
mod removal;
mod symbols;
mod topology;

use self::context::PlanningContext;

pub(crate) struct PlannedGraphCommand {
    pub(crate) transaction: GraphTransaction,
    pub(crate) footprint: GraphMutationFootprint,
    pub(crate) graph: Graph,
    pub(crate) planning_stats: GraphCommandPlanningStats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GraphCommandPlanningStats {
    pub(crate) fact_reads: u64,
    pub(crate) index_bucket_reads: u64,
    pub(crate) index_entries_read: u64,
    pub(crate) operations_emitted: u64,
    pub(crate) full_type_revalidations: u64,
}

impl GraphCommandPlanningStats {
    fn read_facts(&mut self, count: usize) {
        self.fact_reads = self.fact_reads.saturating_add(count as u64);
    }

    fn read_index_bucket(&mut self) {
        self.index_bucket_reads = self.index_bucket_reads.saturating_add(1);
    }

    fn read_index_entries(&mut self, count: usize) {
        self.index_entries_read = self.index_entries_read.saturating_add(count as u64);
    }

    fn emit_operations(&mut self, count: usize) {
        self.operations_emitted = self.operations_emitted.saturating_add(count as u64);
    }

    fn record_full_type_revalidation(&mut self) {
        self.full_type_revalidations = self.full_type_revalidations.saturating_add(1);
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GraphCommandPlanningError {
    #[error(transparent)]
    Mutation(#[from] GraphMutationError),
    #[error("node is missing: {0:?}")]
    MissingNode(NodeId),
    #[error("node appears more than once in one geometry command: {0:?}")]
    DuplicateNodeGeometry(NodeId),
    #[error("port is missing: {0:?}")]
    MissingPort(PortId),
    #[error("duplicate port in replacement topology: {0:?}")]
    DuplicatePort(PortId),
    #[error("port {port:?} belongs to {actual:?}, not {expected:?}")]
    PortOwnerMismatch {
        port: PortId,
        expected: NodeId,
        actual: NodeId,
    },
    #[error("stable port {port:?} changed immutable identity fields")]
    PortIdentityChanged { port: PortId },
    #[error("port has incident edges and removal policy rejects cascading: {0:?}")]
    PortConnected(PortId),
    #[error("node has incident edges and removal policy rejects cascading: {0:?}")]
    NodeConnected(NodeId),
    #[error("node identity is added and removed within one atomic topology edit: {0:?}")]
    NodeLifecycleConflict(NodeId),
    #[error(
        "node {node:?} uses non-canonical registered kind/version {supplied_kind:?}@{supplied_version}; expected {canonical_kind:?}@{canonical_version}"
    )]
    NonCanonicalNodeKind {
        node: NodeId,
        supplied_kind: NodeKindKey,
        supplied_version: u32,
        canonical_kind: NodeKindKey,
        canonical_version: u32,
    },
    #[error("port identity is removed and re-added within one atomic topology edit: {0:?}")]
    PortLifecycleConflict(PortId),
    #[error("edge identity is removed and re-added within one atomic topology edit: {0:?}")]
    EdgeLifecycleConflict(EdgeId),
    #[error("symbol already exists: {0:?}")]
    SymbolAlreadyExists(SymbolId),
    #[error("symbol is missing: {0:?}")]
    MissingSymbol(SymbolId),
    #[error(
        "symbol is referenced by nodes and cannot be removed: {symbol:?}; referrers={referrers:?}"
    )]
    SymbolReferenced {
        symbol: SymbolId,
        referrers: Vec<NodeId>,
    },
    #[error("symbol identity is added and removed within one atomic edit: {0:?}")]
    SymbolLifecycleConflict(SymbolId),
    #[error("graph profile declared a missing port-type dependency: {0:?}")]
    MissingProfileTypeDependency(PortId),
    #[error("graph connection command was rejected: {message}")]
    RuleRejected {
        message: String,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("authoritative command planning produced an invalid operation: {0}")]
    Apply(#[from] crate::graph_kernel::ApplyError),
    #[error(transparent)]
    Profile(#[from] ApplyPipelineError),
}

pub(crate) fn plan_graph_command(
    graph: &Graph,
    index: &GraphIndex,
    catalog: &NodeCatalog,
    command: &GraphCommand,
    profile: Option<&dyn GraphProfile>,
    policy: &GraphRuntimePolicy,
) -> Result<PlannedGraphCommand, GraphCommandPlanningError> {
    let mut context = PlanningContext::new(graph, index, catalog);

    match &command.action {
        GraphCommandAction::Undo | GraphCommandAction::Redo => {
            unreachable!("history commands are replayed by GraphRuntime")
        }
        GraphCommandAction::SetNodeData { node, data } => {
            plan_set_node_data(&mut context, *node, data, profile.is_some())?;
        }
        GraphCommandAction::SetNodeGeometry(edits) => {
            let mut edited_nodes = BTreeSet::new();
            let mut geometry_operations = Vec::new();
            for edit in edits {
                if !edited_nodes.insert(edit.node()) {
                    return Err(GraphCommandPlanningError::DuplicateNodeGeometry(
                        edit.node(),
                    ));
                }
                context.read_facts(1);
                let current = context
                    .graph()
                    .node(&edit.node())
                    .ok_or(GraphCommandPlanningError::MissingNode(edit.node()))?;
                if let Some(position) = edit.position()
                    && current.pos != position
                {
                    geometry_operations.push(GraphOp::SetNodePos {
                        id: edit.node(),
                        from: current.pos,
                        to: position,
                    });
                }
                if let Some(size) = edit.size()
                    && current.size != size
                {
                    geometry_operations.push(GraphOp::SetNodeSize {
                        id: edit.node(),
                        from: current.size,
                        to: size,
                    });
                }
            }
            context.stage(geometry_operations)?;
        }
        GraphCommandAction::SetPortData { port, data } => {
            context.read_facts(1);
            let current = context
                .graph()
                .port(port)
                .ok_or(GraphCommandPlanningError::MissingPort(*port))?;
            let operation = GraphOp::SetPortData {
                id: *port,
                from: current.data.clone(),
                to: data.clone(),
            };
            context.stage([operation])?;
            if profile.is_some() {
                context.schedule_type_revalidation([*port], IncidentEdgePolicy::Reject);
            }
        }
        GraphCommandAction::Symbols(edit) => symbols::plan_symbol_edit(&mut context, edit)?,
        GraphCommandAction::Topology(edit) => {
            topology::plan_topology_edit(&mut context, edit, profile, policy)?;
        }
    }

    if let Some(profile) = profile {
        profile::converge_profile(&mut context, graph, profile, policy)?;
    } else {
        profile::revalidate_unprofiled(&mut context)?;
    }

    let (graph, operations, planning_stats) = context.into_parts();
    let transaction = GraphTransactionGate::normalize_and_validate(GraphTransaction::from_parts(
        command.label.clone(),
        operations,
    ))?;
    let footprint = transaction.footprint();
    Ok(PlannedGraphCommand {
        transaction,
        footprint,
        graph,
        planning_stats,
    })
}

fn plan_set_node_data(
    context: &mut PlanningContext<'_>,
    node: NodeId,
    data: &serde_json::Value,
    revalidate_types: bool,
) -> Result<(), GraphCommandPlanningError> {
    context.read_facts(1);
    let current = context
        .graph()
        .node(&node)
        .ok_or(GraphCommandPlanningError::MissingNode(node))?;
    let affected_ports = current.ports.clone();
    let operation = GraphOp::SetNodeData {
        id: node,
        from: current.data.clone(),
        to: data.clone(),
    };
    context.stage([operation])?;
    if revalidate_types {
        context.schedule_type_revalidation(affected_ports, IncidentEdgePolicy::Reject);
    }
    Ok(())
}

fn stricter_incident_policy(
    left: IncidentEdgePolicy,
    right: IncidentEdgePolicy,
) -> IncidentEdgePolicy {
    if left == IncidentEdgePolicy::Reject || right == IncidentEdgePolicy::Reject {
        IncidentEdgePolicy::Reject
    } else {
        IncidentEdgePolicy::Cascade
    }
}

fn rule_rejected(
    fallback_message: impl Into<String>,
    diagnostics: Vec<Diagnostic>,
) -> GraphCommandPlanningError {
    let message = diagnostics
        .first()
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| fallback_message.into());
    GraphCommandPlanningError::RuleRejected {
        message,
        diagnostics,
    }
}
