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
    if inserted.node_id == source_node || inserted.node_id == target_node {
        return Err(ConnectPlan::reject(
            "inserted node id must be distinct from endpoints",
        ));
    }
    if graph.nodes.contains_key(&inserted.node_id) {
        return Err(ConnectPlan::reject(format!(
            "node already exists: {:?}",
            inserted.node_id
        )));
    }
    for (port_id, _) in &inserted.ports {
        if graph.ports.contains_key(port_id) {
            return Err(ConnectPlan::reject(format!(
                "port already exists: {port_id:?}"
            )));
        }
    }

    if inserted.input == inserted.output {
        return Err(ConnectPlan::reject(
            "inserted input/output ports must be distinct",
        ));
    }

    let (inserted_in, inserted_out) = validate_inserted_ports(inserted, expected_port_kind)?;

    if inserted_in.dir != PortDirection::In || inserted_out.dir != PortDirection::Out {
        return Err(ConnectPlan::reject("inserted ports must be in -> out"));
    }

    Ok(ValidatedInsertNodeSpec {
        input: inserted.input,
        output: inserted.output,
    })
}

fn validate_inserted_ports(
    inserted: &InsertNodeSpec,
    expected_port_kind: PortKind,
) -> Result<(&Port, &Port), ConnectPlan> {
    let mut inserted_in: Option<&Port> = None;
    let mut inserted_out: Option<&Port> = None;
    for (port_id, port) in &inserted.ports {
        if port.node != inserted.node_id {
            return Err(ConnectPlan::reject(format!(
                "inserted port has wrong node: port={port_id:?} expected={:?} got={:?}",
                inserted.node_id, port.node
            )));
        }
        if port.kind != expected_port_kind {
            return Err(ConnectPlan::reject(format!(
                "inserted port kind is incompatible: port={port_id:?} kind={:?} expected={:?}",
                port.kind, expected_port_kind
            )));
        }
        if *port_id == inserted.input {
            inserted_in = Some(port);
        }
        if *port_id == inserted.output {
            inserted_out = Some(port);
        }
    }

    let Some(inserted_in) = inserted_in else {
        return Err(ConnectPlan::reject(
            "inserted input port is missing from spec",
        ));
    };
    let Some(inserted_out) = inserted_out else {
        return Err(ConnectPlan::reject(
            "inserted output port is missing from spec",
        ));
    };

    Ok((inserted_in, inserted_out))
}
