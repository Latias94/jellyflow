use crate::rules::{ConnectPlan, InsertNodeSpec};
use jellyflow_core::core::{Graph, NodeId, Port, PortDirection, PortId, PortKind};

pub(in crate::rules::connection) struct ValidatedInsertNodeSpec {
    pub input: PortId,
    pub output: PortId,
}

pub(in crate::rules::connection) fn validate_insert_node_spec(
    graph: &Graph,
    inserted: &InsertNodeSpec,
    source_node: NodeId,
    target_node: NodeId,
    expected_port_kind: PortKind,
) -> Result<ValidatedInsertNodeSpec, ConnectPlan> {
    InsertNodeSpecValidator {
        graph,
        inserted,
        source_node,
        target_node,
        expected_port_kind,
    }
    .validate()
}

struct InsertNodeSpecValidator<'a> {
    graph: &'a Graph,
    inserted: &'a InsertNodeSpec,
    source_node: NodeId,
    target_node: NodeId,
    expected_port_kind: PortKind,
}

struct InsertedPortRoles<'a> {
    input: &'a Port,
    output: &'a Port,
}

impl InsertNodeSpecValidator<'_> {
    fn validate(&self) -> Result<ValidatedInsertNodeSpec, ConnectPlan> {
        self.ensure_inserted_node_is_distinct()?;
        self.ensure_inserted_ids_are_available()?;
        self.ensure_roles_are_distinct()?;

        let roles = self.validate_ports()?;
        self.ensure_role_directions(&roles)?;

        Ok(ValidatedInsertNodeSpec {
            input: self.inserted.input,
            output: self.inserted.output,
        })
    }

    fn ensure_inserted_node_is_distinct(&self) -> Result<(), ConnectPlan> {
        if self.inserted.node_id == self.source_node || self.inserted.node_id == self.target_node {
            return Err(ConnectPlan::reject(
                "inserted node id must be distinct from endpoints",
            ));
        }
        Ok(())
    }

    fn ensure_inserted_ids_are_available(&self) -> Result<(), ConnectPlan> {
        if self.graph.nodes().contains_key(&self.inserted.node_id) {
            return Err(ConnectPlan::reject(format!(
                "node already exists: {:?}",
                self.inserted.node_id
            )));
        }
        for (port_id, _) in &self.inserted.ports {
            if self.graph.ports().contains_key(port_id) {
                return Err(ConnectPlan::reject(format!(
                    "port already exists: {port_id:?}"
                )));
            }
        }
        Ok(())
    }

    fn ensure_roles_are_distinct(&self) -> Result<(), ConnectPlan> {
        if self.inserted.input == self.inserted.output {
            return Err(ConnectPlan::reject(
                "inserted input/output ports must be distinct",
            ));
        }
        Ok(())
    }

    fn validate_ports(&self) -> Result<InsertedPortRoles<'_>, ConnectPlan> {
        let mut inserted_in: Option<&Port> = None;
        let mut inserted_out: Option<&Port> = None;
        for (port_id, port) in &self.inserted.ports {
            self.validate_port_owner(*port_id, port)?;
            self.validate_port_kind(*port_id, port)?;

            if *port_id == self.inserted.input {
                inserted_in = Some(port);
            }
            if *port_id == self.inserted.output {
                inserted_out = Some(port);
            }
        }

        let Some(input) = inserted_in else {
            return Err(ConnectPlan::reject(
                "inserted input port is missing from spec",
            ));
        };
        let Some(output) = inserted_out else {
            return Err(ConnectPlan::reject(
                "inserted output port is missing from spec",
            ));
        };

        Ok(InsertedPortRoles { input, output })
    }

    fn validate_port_owner(&self, port_id: PortId, port: &Port) -> Result<(), ConnectPlan> {
        if port.node != self.inserted.node_id {
            return Err(ConnectPlan::reject(format!(
                "inserted port has wrong node: port={port_id:?} expected={:?} got={:?}",
                self.inserted.node_id, port.node
            )));
        }
        Ok(())
    }

    fn validate_port_kind(&self, port_id: PortId, port: &Port) -> Result<(), ConnectPlan> {
        if port.kind != self.expected_port_kind {
            return Err(ConnectPlan::reject(format!(
                "inserted port kind is incompatible: port={port_id:?} kind={:?} expected={:?}",
                port.kind, self.expected_port_kind
            )));
        }
        Ok(())
    }

    fn ensure_role_directions(&self, roles: &InsertedPortRoles<'_>) -> Result<(), ConnectPlan> {
        if roles.input.dir != PortDirection::In || roles.output.dir != PortDirection::Out {
            return Err(ConnectPlan::reject("inserted ports must be in -> out"));
        }
        Ok(())
    }
}
