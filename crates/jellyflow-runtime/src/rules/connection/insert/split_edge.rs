use crate::rules::{ConnectPlan, InsertNodeSpec};
use jellyflow_core::core::{EdgeId, Graph, PortDirection};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationBatchPlanner, GraphOp};

use super::super::common::{
    edge_like, port_kind_for_edge_kind, reject_mutation_error, validate_insert_node_spec,
};

/// Plans splitting an existing edge by inserting a node (preserving the edge identity for the first segment).
pub fn plan_split_edge_by_inserting_node(
    graph: &Graph,
    edge_id: EdgeId,
    new_edge_id: EdgeId,
    inserted: InsertNodeSpec,
) -> ConnectPlan {
    let Some(edge) = graph.edges.get(&edge_id) else {
        return ConnectPlan::reject(format!("missing edge: {edge_id:?}"));
    };
    if graph.edges.contains_key(&new_edge_id) {
        return ConnectPlan::reject(format!("edge already exists: {new_edge_id:?}"));
    }

    let Some(from_port) = graph.ports.get(&edge.from) else {
        return ConnectPlan::reject("missing edge.from port");
    };
    let Some(to_port) = graph.ports.get(&edge.to) else {
        return ConnectPlan::reject("missing edge.to port");
    };

    if from_port.dir != PortDirection::Out || to_port.dir != PortDirection::In {
        return ConnectPlan::reject("edge must be out -> in");
    }

    let expected_port_kind = port_kind_for_edge_kind(edge.kind);
    if from_port.kind != expected_port_kind || to_port.kind != expected_port_kind {
        return ConnectPlan::reject("edge kind is incompatible with ports");
    }

    let inserted_ports = match validate_insert_node_spec(
        graph,
        &inserted,
        from_port.node,
        to_port.node,
        expected_port_kind,
    ) {
        Ok(inserted_ports) => inserted_ports,
        Err(plan) => return plan,
    };

    let mut ops: Vec<GraphOp> = Vec::new();

    let mut batch = GraphMutationBatchPlanner::new(graph);
    if let Err(error) = batch.add_node_with_ports(inserted.node_id, inserted.node, inserted.ports) {
        return reject_mutation_error(error);
    }
    if let Err(error) = batch.set_edge_endpoints(
        edge_id,
        EdgeEndpoints {
            from: edge.from,
            to: inserted_ports.input,
        },
    ) {
        return reject_mutation_error(error);
    }
    if let Err(error) = batch.add_edge(new_edge_id, edge_like(edge, inserted_ports.output, edge.to))
    {
        return reject_mutation_error(error);
    }
    ops.extend(batch.into_ops());

    ConnectPlan::from_ops(ops)
}
