use std::collections::BTreeSet;

use crate::graph_kernel::{GraphMutationError, GraphOp};
use jellyflow_core::core::{
    Binding, BindingId, Edge, EdgeId, Graph, GraphLocalBindingTarget, NodeId, PortId,
};

use crate::runtime::commands::IncidentEdgePolicy;

use super::GraphCommandPlanningError;
use super::context::PlanningQuery;

pub(super) fn plan_remove_node(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    id: NodeId,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphOp, GraphCommandPlanningError> {
    query.read_facts(1);
    let node = graph
        .node(&id)
        .cloned()
        .ok_or(GraphCommandPlanningError::MissingNode(id))?;
    let ports = node
        .ports
        .iter()
        .map(|port_id| {
            query.read_facts(1);
            graph
                .port(port_id)
                .cloned()
                .map(|port| (*port_id, port))
                .ok_or(GraphMutationError::MissingPort(*port_id).into())
        })
        .collect::<Result<Vec<_>, GraphCommandPlanningError>>()?;
    let edge_ids = node
        .ports
        .iter()
        .flat_map(|port| query.incident_edges(*port))
        .collect::<BTreeSet<_>>();
    if incident_edges == IncidentEdgePolicy::Reject && !edge_ids.is_empty() {
        return Err(GraphCommandPlanningError::NodeConnected(id));
    }
    let edges = edge_snapshots(graph, &edge_ids, query)?;
    let targets = [GraphLocalBindingTarget::node(id)]
        .into_iter()
        .chain(
            ports
                .iter()
                .map(|(id, _)| GraphLocalBindingTarget::port(*id)),
        )
        .chain(
            edges
                .iter()
                .map(|(id, _)| GraphLocalBindingTarget::edge(*id)),
        );
    let bindings = binding_snapshots(graph, query, targets)?;
    Ok(GraphOp::RemoveNode {
        id,
        node,
        ports,
        edges,
        bindings,
    })
}

pub(super) fn plan_remove_port(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    id: PortId,
    incident_edges: IncidentEdgePolicy,
) -> Result<Vec<GraphOp>, GraphCommandPlanningError> {
    if query.was_port_added(id) {
        return Err(GraphCommandPlanningError::PortLifecycleConflict(id));
    }
    query.read_facts(1);
    let port = graph
        .port(&id)
        .cloned()
        .ok_or(GraphMutationError::MissingPort(id))?;
    let edge_ids = query.incident_edges(id);
    if incident_edges == IncidentEdgePolicy::Reject && !edge_ids.is_empty() {
        return Err(GraphCommandPlanningError::PortConnected(id));
    }
    let edges = edge_snapshots(graph, &edge_ids, query)?;
    let targets = [GraphLocalBindingTarget::port(id)].into_iter().chain(
        edges
            .iter()
            .map(|(id, _)| GraphLocalBindingTarget::edge(*id)),
    );
    let bindings = binding_snapshots(graph, query, targets)?;
    query.read_facts(1);
    let owner = graph
        .node(&port.node)
        .ok_or(GraphCommandPlanningError::MissingNode(port.node))?;
    let from = owner.ports.clone();
    let to = from.iter().copied().filter(|port| *port != id).collect();
    Ok(vec![
        GraphOp::SetNodePorts {
            id: port.node,
            from,
            to,
        },
        GraphOp::RemovePort {
            id,
            port,
            edges,
            bindings,
        },
    ])
}

pub(super) fn plan_remove_edge(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    id: EdgeId,
) -> Result<GraphOp, GraphCommandPlanningError> {
    if query.was_edge_added(id) {
        return Err(GraphCommandPlanningError::EdgeLifecycleConflict(id));
    }
    query.read_facts(1);
    let edge = graph
        .edge(&id)
        .cloned()
        .ok_or(GraphMutationError::MissingEdge(id))?;
    let bindings = binding_snapshots(graph, query, [GraphLocalBindingTarget::edge(id)])?;
    Ok(GraphOp::RemoveEdge { id, edge, bindings })
}

fn edge_snapshots(
    graph: &Graph,
    ids: &BTreeSet<EdgeId>,
    query: &mut PlanningQuery<'_, '_>,
) -> Result<Vec<(EdgeId, Edge)>, GraphCommandPlanningError> {
    ids.iter()
        .map(|id| {
            query.read_facts(1);
            graph
                .edge(id)
                .cloned()
                .map(|edge| (*id, edge))
                .ok_or_else(|| GraphMutationError::MissingEdge(*id).into())
        })
        .collect()
}

fn binding_snapshots(
    graph: &Graph,
    query: &mut PlanningQuery<'_, '_>,
    targets: impl IntoIterator<Item = GraphLocalBindingTarget>,
) -> Result<Vec<(BindingId, Binding)>, GraphCommandPlanningError> {
    query
        .bindings_for_targets(targets)
        .into_iter()
        .map(|id| {
            query.read_facts(1);
            graph
                .binding(&id)
                .cloned()
                .map(|binding| (id, binding))
                .ok_or_else(|| GraphMutationError::MissingBinding(id).into())
        })
        .collect()
}
