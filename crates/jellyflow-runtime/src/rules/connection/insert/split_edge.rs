use crate::rules::{ConnectPlan, InsertNodeSpec};
use jellyflow_core::core::{Edge, EdgeId, Graph, Port, PortDirection, PortKind};
use jellyflow_core::ops::EdgeEndpoints;

use super::super::common::{
    edge_like, ensure_edge_id_available, reject_edge_kind_incompatible, reject_missing_edge,
    validate_insert_node_spec,
};
use super::batch::InsertNodeMutationBuilder;

/// Plans splitting an existing edge by inserting a node (preserving the edge identity for the first segment).
pub fn plan_split_edge_by_inserting_node(
    graph: &Graph,
    edge_id: EdgeId,
    new_edge_id: EdgeId,
    inserted: InsertNodeSpec,
) -> ConnectPlan {
    let split_edge = match SplittableEdge::resolve(graph, edge_id) {
        Ok(split_edge) => split_edge,
        Err(plan) => return plan,
    };
    if let Err(reject) = ensure_edge_id_available(graph, new_edge_id) {
        return reject;
    }

    let inserted_ports = match validate_insert_node_spec(
        graph,
        &inserted,
        split_edge.from_port.node,
        split_edge.to_port.node,
        split_edge.expected_port_kind(),
    ) {
        Ok(inserted_ports) => inserted_ports,
        Err(plan) => return plan,
    };

    let mut batch = match InsertNodeMutationBuilder::new(graph, inserted) {
        Ok(batch) => batch,
        Err(plan) => return plan,
    };
    if let Err(plan) = batch.set_edge_endpoints(
        edge_id,
        EdgeEndpoints::new(split_edge.edge.from, inserted_ports.input),
    ) {
        return plan;
    }
    if let Err(plan) = batch.add_edge(
        new_edge_id,
        edge_like(split_edge.edge, inserted_ports.output, split_edge.edge.to),
    ) {
        return plan;
    }

    ConnectPlan::from_ops(batch.into_ops())
}

struct SplittableEdge<'a> {
    edge: &'a Edge,
    from_port: &'a Port,
    to_port: &'a Port,
}

impl<'a> SplittableEdge<'a> {
    fn resolve(graph: &'a Graph, edge_id: EdgeId) -> Result<Self, ConnectPlan> {
        let Some(edge) = graph.edges.get(&edge_id) else {
            return Err(reject_missing_edge(edge_id));
        };
        let Some(from_port) = graph.ports.get(&edge.from) else {
            return Err(ConnectPlan::reject("missing edge.from port"));
        };
        let Some(to_port) = graph.ports.get(&edge.to) else {
            return Err(ConnectPlan::reject("missing edge.to port"));
        };

        let split_edge = Self {
            edge,
            from_port,
            to_port,
        };
        split_edge.ensure_direction()?;
        split_edge.ensure_port_kinds()?;

        Ok(split_edge)
    }

    fn expected_port_kind(&self) -> PortKind {
        self.edge.kind.port_kind()
    }

    fn ensure_direction(&self) -> Result<(), ConnectPlan> {
        if self.from_port.dir != PortDirection::Out || self.to_port.dir != PortDirection::In {
            return Err(ConnectPlan::reject("edge must be out -> in"));
        }
        Ok(())
    }

    fn ensure_port_kinds(&self) -> Result<(), ConnectPlan> {
        let expected_port_kind = self.expected_port_kind();
        if self.from_port.kind != expected_port_kind || self.to_port.kind != expected_port_kind {
            return Err(reject_edge_kind_incompatible());
        }
        Ok(())
    }
}
