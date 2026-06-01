use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, InsertNodeSpec};
use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphMutationBatchPlanner;

use super::super::common::{
    ConnectionOpBuilder, edge_between, ensure_edge_id_available, reject_mutation_error,
    resolve_policy_checked_connection, validate_insert_node_spec,
};

/// Plans connecting two ports by inserting a node between them.
///
/// This is intended for "auto-fix" workflows like inserting a conversion node when types mismatch.
pub fn plan_connect_by_inserting_node_with_policy(
    graph: &Graph,
    a: PortId,
    b: PortId,
    first_edge_id: EdgeId,
    second_edge_id: EdgeId,
    inserted: InsertNodeSpec,
    state: &NodeGraphInteractionState,
) -> ConnectPlan {
    let endpoints = match resolve_policy_checked_connection(
        graph,
        a,
        b,
        NodeGraphConnectionMode::Strict,
        state,
    ) {
        Ok(endpoints) => endpoints,
        Err(plan) => return plan,
    };

    if let Err(reject) = ensure_edge_id_available(graph, first_edge_id) {
        return reject;
    }
    if let Err(reject) = ensure_edge_id_available(graph, second_edge_id) {
        return reject;
    }

    let expected_port_kind = endpoints.edge_kind.port_kind();
    let inserted_ports = match validate_insert_node_spec(
        graph,
        &inserted,
        endpoints.from.node,
        endpoints.to.node,
        expected_port_kind,
    ) {
        Ok(inserted_ports) => inserted_ports,
        Err(plan) => return plan,
    };

    let mut ops = ConnectionOpBuilder::with_endpoint_capacity_disconnects(graph, &endpoints, None);

    let mut batch = GraphMutationBatchPlanner::new(graph);
    if let Err(error) = batch.add_node_with_ports(inserted.node_id, inserted.node, inserted.ports) {
        return reject_mutation_error(error);
    }
    if let Err(error) = batch.add_edge(
        first_edge_id,
        edge_between(endpoints.edge_kind, endpoints.from_id, inserted_ports.input),
    ) {
        return reject_mutation_error(error);
    }
    if let Err(error) = batch.add_edge(
        second_edge_id,
        edge_between(endpoints.edge_kind, inserted_ports.output, endpoints.to_id),
    ) {
        return reject_mutation_error(error);
    }
    ops.extend(batch.into_ops());

    ConnectPlan::from_ops(ops.into_ops())
}

/// Plans connecting two ports by inserting a node with default interaction policy.
pub fn plan_connect_by_inserting_node(
    graph: &Graph,
    a: PortId,
    b: PortId,
    first_edge_id: EdgeId,
    second_edge_id: EdgeId,
    inserted: InsertNodeSpec,
) -> ConnectPlan {
    plan_connect_by_inserting_node_with_policy(
        graph,
        a,
        b,
        first_edge_id,
        second_edge_id,
        inserted,
        &NodeGraphInteractionState::default(),
    )
}
