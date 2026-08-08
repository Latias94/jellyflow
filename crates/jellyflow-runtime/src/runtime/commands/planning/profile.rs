use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::graph_kernel::{GraphMutationFootprint, GraphOp};
use jellyflow_core::core::{Graph, PortId};

use crate::profile::{
    ApplyPipelineError, GraphProfile, PortTypeImpactContext, PortTypeRevalidation,
    resolved_port_type, type_compatibility_or_default, validate_profile_graph,
};
use crate::rules::edge_type_compatibility_diagnostic;
use crate::runtime::commands::IncidentEdgePolicy;
use crate::runtime::policy::GraphRuntimePolicy;

use super::context::PlanningContext;
use super::removal::plan_remove_edge;
use super::topology::plan_topology_edit;
use super::{GraphCommandPlanningError, rule_rejected, stricter_incident_policy};

pub(super) fn converge_profile(
    context: &mut PlanningContext<'_>,
    before: &Graph,
    profile: &dyn GraphProfile,
    policy: &GraphRuntimePolicy,
) -> Result<(), GraphCommandPlanningError> {
    let bound = profile.concretize_bound();
    for _ in 0..bound {
        let edit = profile.concretize(context.graph());
        if edit.is_empty() {
            let seed_from_mutation = context
                .operations()
                .iter()
                .any(is_non_port_semantic_mutation);
            if let Some(policy) = context
                .type_revalidation_policy()
                .or(seed_from_mutation.then_some(IncidentEdgePolicy::Reject))
            {
                let mut scheduled = context.take_type_revalidations();
                extend_profile_type_revalidations(
                    context,
                    profile,
                    before,
                    policy,
                    seed_from_mutation,
                    &mut scheduled,
                )?;
                if revalidate_incident_edge_types(context, scheduled, Some(profile))? {
                    continue;
                }
            }
            validate_profile_graph(profile, context.graph())?;
            return Ok(());
        }
        plan_topology_edit(context, &edit, Some(profile), policy)?;
    }
    Err(ApplyPipelineError::ConcretizeNonConvergent { bound }.into())
}

pub(super) fn revalidate_unprofiled(
    context: &mut PlanningContext<'_>,
) -> Result<(), GraphCommandPlanningError> {
    let scheduled = context.take_type_revalidations();
    revalidate_incident_edge_types(context, scheduled, None)?;
    Ok(())
}

fn revalidate_incident_edge_types(
    context: &mut PlanningContext<'_>,
    ports: BTreeMap<PortId, IncidentEdgePolicy>,
    profile: Option<&dyn GraphProfile>,
) -> Result<bool, GraphCommandPlanningError> {
    let mut edge_policies = BTreeMap::new();
    for (port, policy) in ports {
        for edge in context.incident_edges(port) {
            edge_policies
                .entry(edge)
                .and_modify(|current| *current = stricter_incident_policy(*current, policy))
                .or_insert(policy);
        }
    }
    let mut removed_edges = false;
    let compatibility = type_compatibility_or_default(profile);
    for (edge_id, incident_edges) in edge_policies {
        let Some(edge) = context.graph().edge(&edge_id).cloned() else {
            continue;
        };
        let affected_ports = [edge.from, edge.to];
        context.read_facts(3);
        let diagnostic = edge_type_compatibility_diagnostic(
            context.graph(),
            edge_id,
            |graph, port| resolved_port_type(profile, graph, port),
            compatibility,
        );
        let Some(diagnostic) = diagnostic else {
            continue;
        };
        if incident_edges == IncidentEdgePolicy::Reject {
            return Err(rule_rejected(diagnostic.message.clone(), vec![diagnostic]));
        }
        let removal = {
            let (graph, mut query) = context.query_parts();
            plan_remove_edge(graph, &mut query, edge_id)?
        };
        context.stage([removal])?;
        context.schedule_type_revalidation(affected_ports, incident_edges);
        removed_edges = true;
    }
    Ok(removed_edges)
}

fn extend_profile_type_revalidations(
    context: &mut PlanningContext<'_>,
    profile: &dyn GraphProfile,
    before: &Graph,
    policy: IncidentEdgePolicy,
    seed_from_mutation: bool,
    scheduled: &mut BTreeMap<PortId, IncidentEdgePolicy>,
) -> Result<(), GraphCommandPlanningError> {
    let mut footprint = GraphMutationFootprint::new();
    for operation in context.operations() {
        operation.append_footprint(&mut footprint);
    }

    let mut local_nodes = footprint.nodes.clone();
    for port in &footprint.ports {
        context.read_facts(1);
        let owner = context.graph().port(port).map(|port| port.node);
        let owner = owner.or_else(|| {
            context.read_facts(1);
            before.port(port).map(|port| port.node)
        });
        if let Some(owner) = owner {
            local_nodes.insert(owner);
        }
    }
    let mut local_ports = BTreeSet::new();
    for node in local_nodes {
        context.read_facts(2);
        if let Some(node) = before.node(&node) {
            local_ports.extend(node.ports.iter().copied());
        }
        if let Some(node) = context.graph().node(&node) {
            local_ports.extend(node.ports.iter().copied());
        }
    }
    for edge in &footprint.edges {
        context.read_facts(2);
        if let Some(edge) = before.edge(edge) {
            local_ports.extend([edge.from, edge.to]);
        }
        if let Some(edge) = context.graph().edge(edge) {
            local_ports.extend([edge.from, edge.to]);
        }
    }
    merge_type_revalidations(scheduled, local_ports, policy);

    let mut pending = scheduled.keys().copied().collect::<VecDeque<_>>();
    let mut expanded = BTreeSet::new();
    let mut requires_all = false;
    {
        let (after, mut query) = context.query_parts();
        let mut impact = PortTypeImpactContext::new(before, after, &footprint, &mut query);
        if seed_from_mutation {
            match profile.port_types_affected_by_mutation(&mut impact) {
                PortTypeRevalidation::All => requires_all = true,
                PortTypeRevalidation::Exact(ports) => {
                    extend_declared_type_revalidations(
                        &mut impact,
                        scheduled,
                        &mut pending,
                        ports,
                        policy,
                    )?;
                }
            }
        }
        while let Some(changed_port) = pending.pop_front() {
            if requires_all {
                break;
            }
            if !expanded.insert(changed_port) {
                continue;
            }
            match profile.dependent_port_types(&mut impact, changed_port) {
                PortTypeRevalidation::All => {
                    requires_all = true;
                    break;
                }
                PortTypeRevalidation::Exact(ports) => {
                    extend_declared_type_revalidations(
                        &mut impact,
                        scheduled,
                        &mut pending,
                        ports,
                        policy,
                    )?;
                }
            }
        }
    }
    if requires_all {
        context.record_full_type_revalidation();
        let port_count = context.graph().ports().len();
        context.read_facts(port_count);
        merge_type_revalidations(scheduled, context.graph().ports().keys().copied(), policy);
    }
    Ok(())
}

fn extend_declared_type_revalidations(
    impact: &mut PortTypeImpactContext<'_>,
    scheduled: &mut BTreeMap<PortId, IncidentEdgePolicy>,
    pending: &mut VecDeque<PortId>,
    ports: impl IntoIterator<Item = PortId>,
    policy: IncidentEdgePolicy,
) -> Result<(), GraphCommandPlanningError> {
    for port in ports {
        let exists = impact.port_after(port).is_some() || impact.port_before(port).is_some();
        if !exists {
            return Err(GraphCommandPlanningError::MissingProfileTypeDependency(
                port,
            ));
        }
        let is_new = !scheduled.contains_key(&port);
        merge_type_revalidations(scheduled, [port], policy);
        if is_new {
            pending.push_back(port);
        }
    }
    Ok(())
}

fn is_non_port_semantic_mutation(operation: &GraphOp) -> bool {
    match operation {
        GraphOp::AddNode { .. }
        | GraphOp::RemoveNode { .. }
        | GraphOp::SetNodeKind { .. }
        | GraphOp::SetNodeKindVersion { .. }
        | GraphOp::SetNodeParent { .. }
        | GraphOp::SetNodeHidden { .. }
        | GraphOp::SetNodeCollapsed { .. }
        | GraphOp::SetNodePorts { .. }
        | GraphOp::SetNodeData { .. }
        | GraphOp::AddEdge { .. }
        | GraphOp::RemoveEdge { .. }
        | GraphOp::SetEdgeKind { .. }
        | GraphOp::SetEdgeData { .. }
        | GraphOp::SetEdgeEndpoints { .. }
        | GraphOp::AddImport { .. }
        | GraphOp::RemoveImport { .. }
        | GraphOp::SetImportAlias { .. }
        | GraphOp::AddSymbol { .. }
        | GraphOp::RemoveSymbol { .. }
        | GraphOp::SetSymbolName { .. }
        | GraphOp::SetSymbolType { .. }
        | GraphOp::SetSymbolDefaultValue { .. }
        | GraphOp::SetSymbolMeta { .. }
        | GraphOp::AddBinding { .. }
        | GraphOp::RemoveBinding { .. }
        | GraphOp::SetBindingSubject { .. }
        | GraphOp::SetBindingTarget { .. }
        | GraphOp::SetBindingKind { .. }
        | GraphOp::SetBindingMeta { .. }
        | GraphOp::AddGroup { .. }
        | GraphOp::RemoveGroup { .. }
        | GraphOp::SetGroupTitle { .. }
        | GraphOp::SetGroupColor { .. }
        | GraphOp::AddStickyNote { .. }
        | GraphOp::RemoveStickyNote { .. }
        | GraphOp::SetStickyNoteText { .. }
        | GraphOp::SetStickyNoteColor { .. } => true,
        GraphOp::RemovePort {
            edges, bindings, ..
        } => !edges.is_empty() || !bindings.is_empty(),
        GraphOp::SetNodePos { .. }
        | GraphOp::SetNodeOrigin { .. }
        | GraphOp::SetNodeSelectable { .. }
        | GraphOp::SetNodeDraggable { .. }
        | GraphOp::SetNodeConnectable { .. }
        | GraphOp::SetNodeDeletable { .. }
        | GraphOp::SetNodeExtent { .. }
        | GraphOp::SetNodeExpandParent { .. }
        | GraphOp::SetNodeSize { .. }
        | GraphOp::AddPort { .. }
        | GraphOp::SetPortConnectable { .. }
        | GraphOp::SetPortConnectableStart { .. }
        | GraphOp::SetPortConnectableEnd { .. }
        | GraphOp::SetPortType { .. }
        | GraphOp::SetPortData { .. }
        | GraphOp::SetEdgeSelectable { .. }
        | GraphOp::SetEdgeHidden { .. }
        | GraphOp::SetEdgeInteractionWidth { .. }
        | GraphOp::SetEdgeDeletable { .. }
        | GraphOp::SetEdgeReconnectable { .. }
        | GraphOp::SetEdgeView { .. }
        | GraphOp::SetGroupRect { .. }
        | GraphOp::SetStickyNoteRect { .. } => false,
    }
}

fn merge_type_revalidations(
    scheduled: &mut BTreeMap<PortId, IncidentEdgePolicy>,
    ports: impl IntoIterator<Item = PortId>,
    policy: IncidentEdgePolicy,
) {
    for port in ports {
        scheduled
            .entry(port)
            .and_modify(|current| *current = stricter_incident_policy(*current, policy))
            .or_insert(policy);
    }
}
