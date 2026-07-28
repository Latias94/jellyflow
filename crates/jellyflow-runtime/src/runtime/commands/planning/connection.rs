use std::collections::BTreeSet;

use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{EdgeId, EdgeKind, Graph, NodeId, PortId};

use crate::profile::{GraphProfile, resolved_port_type, type_compatibility_or_default};
use crate::rules::{
    ConnectPlan, ConnectionPlanningContext, Diagnostic,
    plan_connect_typed_with_mode_and_policy_using,
    plan_reconnect_edge_typed_with_mode_and_policy_using, validate_delete_edge_policy,
};
use crate::runtime::policy::GraphRuntimePolicy;

use super::context::{PlanningContext, PlanningQuery};
use super::removal::plan_remove_edge;
use super::{GraphCommandPlanningError, rule_rejected};
use crate::runtime::commands::{ConnectionEdit, IncidentEdgePolicy};

pub(super) fn plan_connection_edit(
    context: &mut PlanningContext<'_>,
    edit: ConnectionEdit,
    profile: Option<&dyn GraphProfile>,
    policy: &GraphRuntimePolicy,
) -> Result<(), GraphCommandPlanningError> {
    match edit {
        ConnectionEdit::Connect {
            edge,
            from,
            to,
            mode,
        } => {
            if context.was_edge_removed(edge) {
                return Err(GraphCommandPlanningError::EdgeLifecycleConflict(edge));
            }
            context.read_facts(1);
            if context.graph().edge(&edge).is_some() {
                return Err(rule_rejected(
                    format!("edge already exists: {edge:?}"),
                    vec![Diagnostic::error(
                        "connect.edge_already_exists",
                        crate::rules::DiagnosticTarget::Edge { id: edge },
                        format!("edge already exists: {edge:?}"),
                    )],
                ));
            }
            let compatibility = type_compatibility_or_default(profile);
            let plan = {
                let (graph, query) = context.query_parts();
                let mut query = IndexedConnectionPlanningContext { query };
                plan_connect_typed_with_mode_and_policy_using(
                    graph,
                    edge,
                    from,
                    to,
                    mode,
                    policy,
                    |graph, port| resolved_port_type(profile, graph, port),
                    compatibility,
                    &mut query,
                )
            };
            let planned = accepted_connect_operations(plan)?;
            let affected_ports = profile
                .is_some()
                .then(|| connection_operation_ports(&planned));
            context.stage(planned)?;
            if let Some(affected_ports) = affected_ports {
                context.schedule_type_revalidation(affected_ports, IncidentEdgePolicy::Reject);
            }
            reject_disallowed_cycle(context, edge, profile, policy)?;
        }
        ConnectionEdit::Disconnect { edge } => {
            if context.was_edge_added(edge) {
                return Err(GraphCommandPlanningError::EdgeLifecycleConflict(edge));
            }
            let planned = {
                let (graph, mut query) = context.query_parts();
                plan_disconnect_operation(graph, &mut query, edge, policy)?
            };
            let affected_ports = profile
                .is_some()
                .then(|| connection_operation_ports(std::slice::from_ref(&planned)));
            context.stage([planned])?;
            if let Some(affected_ports) = affected_ports {
                context.schedule_type_revalidation(affected_ports, IncidentEdgePolicy::Reject);
            }
        }
        ConnectionEdit::Reconnect {
            edge,
            endpoint,
            new_port,
            mode,
        } => {
            context.read_facts(1);
            let compatibility = type_compatibility_or_default(profile);
            let plan = {
                let (graph, query) = context.query_parts();
                let mut query = IndexedConnectionPlanningContext { query };
                plan_reconnect_edge_typed_with_mode_and_policy_using(
                    graph,
                    edge,
                    endpoint,
                    new_port,
                    mode,
                    policy,
                    |graph, port| resolved_port_type(profile, graph, port),
                    compatibility,
                    &mut query,
                )
            };
            let planned = accepted_connect_operations(plan)?;
            let affected_ports = profile
                .is_some()
                .then(|| connection_operation_ports(&planned));
            context.stage(planned)?;
            if let Some(affected_ports) = affected_ports {
                context.schedule_type_revalidation(affected_ports, IncidentEdgePolicy::Reject);
            }
            reject_disallowed_cycle(context, edge, profile, policy)?;
        }
    }
    Ok(())
}

struct IndexedConnectionPlanningContext<'query, 'index> {
    query: PlanningQuery<'query, 'index>,
}

impl ConnectionPlanningContext for IndexedConnectionPlanningContext<'_, '_> {
    fn incident_edge_ids(&mut self, _graph: &Graph, port: PortId) -> Vec<EdgeId> {
        let edges = self.query.incident_edges(port);
        self.query.read_facts(edges.len());
        edges.into_iter().collect()
    }

    fn remove_edge_op(&mut self, graph: &Graph, edge: EdgeId) -> Result<GraphOp, ConnectPlan> {
        plan_remove_edge(graph, &mut self.query, edge).map_err(|error| {
            ConnectPlan::reject_with_diagnostic(Diagnostic::error(
                "connect.planning_failed",
                crate::rules::DiagnosticTarget::Edge { id: edge },
                error.to_string(),
            ))
        })
    }
}

fn connection_operation_ports(operations: &[GraphOp]) -> BTreeSet<PortId> {
    let mut ports = BTreeSet::new();
    for operation in operations {
        match operation {
            GraphOp::AddEdge { edge, .. } | GraphOp::RemoveEdge { edge, .. } => {
                ports.extend([edge.from, edge.to]);
            }
            GraphOp::SetEdgeEndpoints { from, to, .. } => {
                ports.extend([from.from, from.to, to.from, to.to]);
            }
            _ => {}
        }
    }
    ports
}

fn plan_disconnect_operation(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    edge: EdgeId,
    policy: &GraphRuntimePolicy,
) -> Result<GraphOp, GraphCommandPlanningError> {
    query.read_facts(1);
    if let Some(diagnostic) = validate_delete_edge_policy(graph, edge, policy) {
        return Err(rule_rejected(diagnostic.message.clone(), vec![diagnostic]));
    }
    plan_remove_edge(graph, query, edge)
}

fn reject_disallowed_cycle(
    context: &mut PlanningContext<'_>,
    edge_id: EdgeId,
    profile: Option<&dyn GraphProfile>,
    policy: &GraphRuntimePolicy,
) -> Result<(), GraphCommandPlanningError> {
    if profile.is_none()
        && policy.cycles_allowed(EdgeKind::Data)
        && policy.cycles_allowed(EdgeKind::Exec)
    {
        return Ok(());
    }
    context.read_facts(1);
    let Some(edge) = context.graph().edge(&edge_id).cloned() else {
        return Ok(());
    };
    if policy.cycles_allowed(edge.kind)
        && profile.is_none_or(|profile| profile.allow_cycles(edge.kind))
    {
        return Ok(());
    }
    context.read_facts(2);
    let Some(source) = context.graph().port(&edge.from).map(|port| port.node) else {
        return Ok(());
    };
    let Some(target) = context.graph().port(&edge.to).map(|port| port.node) else {
        return Ok(());
    };
    let reaches = {
        let (graph, mut query) = context.query_parts();
        node_reaches(graph, &mut query, target, source, edge.kind, edge_id)
    };
    if !reaches {
        return Ok(());
    }

    Err(rule_rejected(
        "connection would create a disallowed cycle",
        vec![Diagnostic::error(
            "connect.cycle",
            crate::rules::DiagnosticTarget::Edge { id: edge_id },
            "connection would create a disallowed cycle",
        )],
    ))
}

fn node_reaches(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    from: NodeId,
    target: NodeId,
    edge_kind: EdgeKind,
    skipped_edge: EdgeId,
) -> bool {
    let mut pending = vec![from];
    let mut visited_nodes = BTreeSet::new();
    let mut visited_edges = BTreeSet::new();
    while let Some(node_id) = pending.pop() {
        if node_id == target {
            return true;
        }
        if !visited_nodes.insert(node_id) {
            continue;
        }
        query.read_facts(1);
        let Some(node) = graph.node(&node_id) else {
            continue;
        };
        for port_id in &node.ports {
            for edge_id in query.incident_edges(*port_id) {
                if edge_id == skipped_edge || !visited_edges.insert(edge_id) {
                    continue;
                }
                query.read_facts(1);
                let Some(edge) = graph.edge(&edge_id) else {
                    continue;
                };
                if edge.kind != edge_kind || edge.from != *port_id {
                    continue;
                }
                query.read_facts(1);
                if let Some(next) = graph.port(&edge.to).map(|port| port.node) {
                    pending.push(next);
                }
            }
        }
    }
    false
}

fn accepted_connect_operations(
    plan: ConnectPlan,
) -> Result<Vec<GraphOp>, GraphCommandPlanningError> {
    if plan.is_reject() {
        return Err(rule_rejected("connection rejected", plan.diagnostics));
    }
    Ok(plan.into_ops())
}
