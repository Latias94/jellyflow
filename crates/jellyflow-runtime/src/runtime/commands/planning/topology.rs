use std::collections::{BTreeMap, BTreeSet};

use crate::graph_kernel::{GraphMutationError, GraphOp};
use jellyflow_core::core::{Node, NodeId, Port, PortId};

use crate::profile::GraphProfile;
use crate::rules::validate_delete_node_policy;
use crate::runtime::policy::GraphRuntimePolicy;

use super::connection::plan_connection_edit;
use super::context::PlanningContext;
use super::removal::{plan_remove_node, plan_remove_port};
use super::{GraphCommandPlanningError, plan_set_node_data, rule_rejected};
use crate::runtime::commands::model::TopologyOperation;
use crate::runtime::commands::{IncidentEdgePolicy, TopologyEdit, TopologyPort};

pub(super) fn plan_topology_edit(
    context: &mut PlanningContext<'_>,
    edit: &TopologyEdit,
    profile: Option<&dyn GraphProfile>,
    policy: &GraphRuntimePolicy,
) -> Result<(), GraphCommandPlanningError> {
    for operation in &edit.operations {
        plan_topology_operation(context, operation, profile, policy)?;
    }
    Ok(())
}

fn plan_topology_operation(
    context: &mut PlanningContext<'_>,
    operation: &TopologyOperation,
    profile: Option<&dyn GraphProfile>,
    policy: &GraphRuntimePolicy,
) -> Result<(), GraphCommandPlanningError> {
    match operation {
        TopologyOperation::SetNodeData { node, data } => {
            plan_set_node_data(context, *node, data, profile.is_some())?;
        }
        TopologyOperation::AddNode { id, node, ports } => {
            if context.was_node_removed(*id) {
                return Err(GraphCommandPlanningError::NodeLifecycleConflict(*id));
            }
            let planned = plan_add_node(context, *id, node, ports)?;
            context.stage(planned)?;
            if profile.is_some() {
                context.schedule_type_revalidation([], IncidentEdgePolicy::Reject);
            }
        }
        TopologyOperation::RemoveNode { id, incident_edges } => {
            if context.was_node_added(*id) {
                return Err(GraphCommandPlanningError::NodeLifecycleConflict(*id));
            }
            if context.graph().node(id).is_some()
                && let Some(diagnostic) = validate_delete_node_policy(context.graph(), *id, policy)
            {
                return Err(rule_rejected(diagnostic.message.clone(), vec![diagnostic]));
            }
            let planned = {
                let (graph, mut query) = context.query_parts();
                plan_remove_node(graph, &mut query, *id, *incident_edges)?
            };
            context.stage([planned])?;
            if profile.is_some() {
                context.schedule_type_revalidation([], *incident_edges);
            }
        }
        TopologyOperation::ReplaceNodePorts {
            node,
            ports,
            incident_edges,
        } => plan_replace_node_ports(context, *node, ports, *incident_edges, profile)?,
        TopologyOperation::Connection(edit) => {
            plan_connection_edit(context, *edit, profile, policy)?;
        }
    }
    Ok(())
}

fn plan_replace_node_ports(
    context: &mut PlanningContext<'_>,
    node_id: NodeId,
    ports: &[TopologyPort],
    incident_edges: IncidentEdgePolicy,
    profile: Option<&dyn GraphProfile>,
) -> Result<(), GraphCommandPlanningError> {
    context.read_facts(1);
    let current_order = context
        .graph()
        .node(&node_id)
        .ok_or(GraphCommandPlanningError::MissingNode(node_id))?
        .ports
        .clone();
    let desired = collect_desired_ports(node_id, ports)?;
    let desired_ids = desired.keys().copied().collect::<BTreeSet<_>>();
    let mut ports_to_revalidate = BTreeSet::new();

    for port in current_order
        .iter()
        .copied()
        .filter(|port| !desired_ids.contains(port))
    {
        if context.was_port_added(port) {
            return Err(GraphCommandPlanningError::PortLifecycleConflict(port));
        }
        let planned = {
            let (graph, mut query) = context.query_parts();
            plan_remove_port(graph, &mut query, port, incident_edges)?
        };
        context.stage(planned)?;
    }

    for port in ports {
        context.read_facts(1);
        if let Some(current) = context.graph().port(&port.id).cloned() {
            validate_port_owner(port.id, node_id, &port.port)?;
            if current.node != node_id {
                return Err(GraphCommandPlanningError::PortOwnerMismatch {
                    port: port.id,
                    expected: node_id,
                    actual: current.node,
                });
            }
            if port_semantic_identity_changed(&current, &port.port) {
                if context.was_port_added(port.id) || context.was_port_removed(port.id) {
                    return Err(GraphCommandPlanningError::PortLifecycleConflict(port.id));
                }
                let removal = {
                    let (graph, mut query) = context.query_parts();
                    plan_remove_port(graph, &mut query, port.id, incident_edges)?
                };
                context.stage(removal)?;
                context.stage([GraphOp::AddPort {
                    id: port.id,
                    port: port.port.clone(),
                }])?;
                ports_to_revalidate.insert(port.id);
            } else {
                if profile.is_some() || current.ty != port.port.ty || current.data != port.port.data
                {
                    ports_to_revalidate.insert(port.id);
                }
                context.stage(port_updates(port.id, &current, &port.port))?;
            }
        } else {
            if context.was_port_removed(port.id) {
                return Err(GraphCommandPlanningError::PortLifecycleConflict(port.id));
            }
            context.stage([GraphOp::AddPort {
                id: port.id,
                port: port.port.clone(),
            }])?;
        }
    }

    let final_order = ports.iter().map(|port| port.id).collect::<Vec<_>>();
    context.read_facts(1);
    let staged_order = context
        .graph()
        .node(&node_id)
        .ok_or(GraphCommandPlanningError::MissingNode(node_id))?
        .ports
        .clone();
    if staged_order != final_order {
        context.stage([GraphOp::SetNodePorts {
            id: node_id,
            from: staged_order,
            to: final_order,
        }])?;
    }
    context.schedule_type_revalidation(ports_to_revalidate, incident_edges);
    Ok(())
}

fn collect_desired_ports(
    node: NodeId,
    ports: &[TopologyPort],
) -> Result<BTreeMap<PortId, Port>, GraphCommandPlanningError> {
    let mut desired = BTreeMap::new();
    for port in ports {
        if port.port.node != node {
            return Err(GraphCommandPlanningError::PortOwnerMismatch {
                port: port.id,
                expected: node,
                actual: port.port.node,
            });
        }
        if desired.insert(port.id, port.port.clone()).is_some() {
            return Err(GraphCommandPlanningError::DuplicatePort(port.id));
        }
    }
    Ok(desired)
}

fn validate_new_ports(
    node: NodeId,
    ports: &[TopologyPort],
) -> Result<Vec<(PortId, Port)>, GraphCommandPlanningError> {
    collect_desired_ports(node, ports)?;
    Ok(ports
        .iter()
        .map(|port| (port.id, port.port.clone()))
        .collect())
}

fn validate_port_owner(
    port_id: PortId,
    node: NodeId,
    desired: &Port,
) -> Result<(), GraphCommandPlanningError> {
    if desired.node != node {
        return Err(GraphCommandPlanningError::PortOwnerMismatch {
            port: port_id,
            expected: node,
            actual: desired.node,
        });
    }
    Ok(())
}

fn port_semantic_identity_changed(current: &Port, desired: &Port) -> bool {
    current.node != desired.node
        || current.key != desired.key
        || current.dir != desired.dir
        || current.kind != desired.kind
        || current.capacity != desired.capacity
}

fn port_updates(id: PortId, current: &Port, desired: &Port) -> Vec<GraphOp> {
    let mut operations = Vec::new();
    if current.connectable != desired.connectable {
        operations.push(GraphOp::SetPortConnectable {
            id,
            from: current.connectable,
            to: desired.connectable,
        });
    }
    if current.connectable_start != desired.connectable_start {
        operations.push(GraphOp::SetPortConnectableStart {
            id,
            from: current.connectable_start,
            to: desired.connectable_start,
        });
    }
    if current.connectable_end != desired.connectable_end {
        operations.push(GraphOp::SetPortConnectableEnd {
            id,
            from: current.connectable_end,
            to: desired.connectable_end,
        });
    }
    if current.ty != desired.ty {
        operations.push(GraphOp::SetPortType {
            id,
            from: current.ty.clone(),
            to: desired.ty.clone(),
        });
    }
    if current.data != desired.data {
        operations.push(GraphOp::SetPortData {
            id,
            from: current.data.clone(),
            to: desired.data.clone(),
        });
    }
    operations
}

fn plan_add_node(
    context: &mut PlanningContext<'_>,
    id: NodeId,
    node: &Node,
    ports: &[TopologyPort],
) -> Result<Vec<GraphOp>, GraphCommandPlanningError> {
    validate_canonical_node_kind(context, id, node)?;
    context.read_facts(1);
    if context.graph().node(&id).is_some() {
        return Err(GraphMutationError::NodeAlreadyExists(id).into());
    }
    if let Some(parent) = node.parent {
        context.read_facts(1);
        if context.graph().group(&parent).is_none() {
            return Err(GraphMutationError::MissingGroup(parent).into());
        }
    }

    let ports = validate_new_ports(id, ports)?;
    for (port_id, _) in &ports {
        if context.was_port_removed(*port_id) {
            return Err(GraphCommandPlanningError::PortLifecycleConflict(*port_id));
        }
        context.read_facts(1);
        if context.graph().port(port_id).is_some() {
            return Err(GraphMutationError::PortAlreadyExists(*port_id).into());
        }
    }

    let order = ports
        .iter()
        .map(|(port_id, _)| *port_id)
        .collect::<Vec<_>>();
    let mut node = node.clone();
    node.ports.clear();
    let mut operations = Vec::with_capacity(2 + ports.len());
    operations.push(GraphOp::AddNode { id, node });
    operations.extend(
        ports
            .into_iter()
            .map(|(id, port)| GraphOp::AddPort { id, port }),
    );
    if !order.is_empty() {
        operations.push(GraphOp::SetNodePorts {
            id,
            from: Vec::new(),
            to: order,
        });
    }
    Ok(operations)
}

fn validate_canonical_node_kind(
    context: &PlanningContext<'_>,
    node_id: NodeId,
    node: &Node,
) -> Result<(), GraphCommandPlanningError> {
    let Some(canonical_kind) = context.catalog().resolve_kind(&node.kind) else {
        return Ok(());
    };
    let spec = context
        .catalog()
        .get(canonical_kind)
        .expect("a resolved canonical kind always has a registered specification");
    if node.kind == *canonical_kind && node.kind_version == spec.latest_kind_version() {
        return Ok(());
    }
    Err(GraphCommandPlanningError::NonCanonicalNodeKind {
        node: node_id,
        supplied_kind: node.kind.clone(),
        supplied_version: node.kind_version,
        canonical_kind: canonical_kind.clone(),
        canonical_version: spec.latest_kind_version(),
    })
}
