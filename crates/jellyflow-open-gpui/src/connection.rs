use jellyflow::{
    core::{
        DefaultTypeCompatibility, EdgeId, Graph, GraphTransaction, NodeGraphConnectionMode, PortId,
    },
    runtime::{
        io::NodeGraphInteractionState,
        rules::{
            Diagnostic, EdgeEndpoint, plan_connect_typed_with_mode_and_policy,
            plan_connect_with_mode_and_policy, plan_delete_edge_with_policy,
            plan_reconnect_edge_with_mode_and_policy,
        },
        runtime::{
            connection::{
                connect_edge_transaction, connect_edge_transaction_with_edge_id,
                reconnect_edge_transaction,
            },
            delete::delete_selection_transaction_from_plan,
        },
    },
};
use thiserror::Error;

/// Widget-free connection edit observed by an Open GPUI host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiConnectionSyncRequest {
    Connect {
        source: PortId,
        target: PortId,
        edge: Option<EdgeId>,
    },
    Reconnect {
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        new_port: PortId,
    },
    Delete {
        edge: EdgeId,
    },
}

/// Runtime-policy failure while converting a host connection edit into a graph transaction.
#[derive(Debug, Error)]
pub enum OpenGpuiConnectionSyncError {
    #[error("connect edge was rejected")]
    ConnectRejected { diagnostics: Vec<Diagnostic> },
    #[error("reconnect edge was rejected")]
    ReconnectRejected { diagnostics: Vec<Diagnostic> },
    #[error("delete edge was rejected")]
    DeleteRejected { diagnostics: Vec<Diagnostic> },
}

/// Plans one host-observed connection edit through Jellyflow's runtime rules.
pub fn plan_connection_sync_transaction(
    graph: &Graph,
    request: OpenGpuiConnectionSyncRequest,
    mode: NodeGraphConnectionMode,
    interaction: &NodeGraphInteractionState,
) -> Result<Option<GraphTransaction>, OpenGpuiConnectionSyncError> {
    match request {
        OpenGpuiConnectionSyncRequest::Connect {
            source,
            target,
            edge,
        } => plan_connection_insert_transaction(graph, source, target, edge, mode, interaction),
        OpenGpuiConnectionSyncRequest::Reconnect {
            edge,
            endpoint,
            new_port,
        } => plan_connection_reconnect_transaction(
            graph,
            edge,
            endpoint,
            new_port,
            mode,
            interaction,
        ),
        OpenGpuiConnectionSyncRequest::Delete { edge } => {
            plan_connection_delete_transaction(graph, edge, interaction)
        }
    }
}

fn plan_connection_insert_transaction(
    graph: &Graph,
    source: PortId,
    target: PortId,
    edge: Option<EdgeId>,
    mode: NodeGraphConnectionMode,
    interaction: &NodeGraphInteractionState,
) -> Result<Option<GraphTransaction>, OpenGpuiConnectionSyncError> {
    let plan = plan_connect_existing_ports(graph, source, target, mode, interaction);
    if plan.is_reject() {
        return Err(OpenGpuiConnectionSyncError::ConnectRejected {
            diagnostics: plan.diagnostics().to_vec(),
        });
    }

    Ok(match edge {
        Some(edge) => connect_edge_transaction_with_edge_id(&plan, edge),
        None => connect_edge_transaction(&plan),
    })
}

fn plan_connection_reconnect_transaction(
    graph: &Graph,
    edge: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
    mode: NodeGraphConnectionMode,
    interaction: &NodeGraphInteractionState,
) -> Result<Option<GraphTransaction>, OpenGpuiConnectionSyncError> {
    let plan = plan_reconnect_edge_with_mode_and_policy(
        graph,
        edge,
        endpoint,
        new_port,
        mode,
        interaction,
    );
    if plan.is_reject() {
        return Err(OpenGpuiConnectionSyncError::ReconnectRejected {
            diagnostics: plan.diagnostics().to_vec(),
        });
    }

    Ok(reconnect_edge_transaction(&plan))
}

fn plan_connection_delete_transaction(
    graph: &Graph,
    edge: EdgeId,
    interaction: &NodeGraphInteractionState,
) -> Result<Option<GraphTransaction>, OpenGpuiConnectionSyncError> {
    let plan = plan_delete_edge_with_policy(graph, edge, interaction);
    if plan.is_reject() {
        return Err(OpenGpuiConnectionSyncError::DeleteRejected {
            diagnostics: plan.diagnostics().to_vec(),
        });
    }

    Ok(delete_selection_transaction_from_plan(plan))
}

fn plan_connect_existing_ports(
    graph: &Graph,
    source: PortId,
    target: PortId,
    mode: NodeGraphConnectionMode,
    interaction: &NodeGraphInteractionState,
) -> jellyflow::runtime::rules::ConnectPlan {
    let has_typed_endpoint = |port_id: PortId| {
        graph
            .ports()
            .get(&port_id)
            .is_some_and(|port| port.ty.is_some())
    };
    if has_typed_endpoint(source) || has_typed_endpoint(target) {
        let mut compat = DefaultTypeCompatibility;
        return plan_connect_typed_with_mode_and_policy(
            graph,
            source,
            target,
            mode,
            interaction,
            |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
            &mut compat,
        );
    }

    plan_connect_with_mode_and_policy(graph, source, target, mode, interaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::core::{
        Edge, EdgeKind, GraphId, GraphOp, Node, NodeId, NodeKindKey, Port, PortCapacity, PortKey,
        PortKind,
    };

    #[test]
    fn connection_sync_connect_uses_runtime_plan_and_optional_edge_id() {
        let fixture = connection_fixture();
        let requested_edge = EdgeId::from_u128(900);
        let transaction = plan_connection_sync_transaction(
            &fixture.graph,
            OpenGpuiConnectionSyncRequest::Connect {
                source: fixture.out_a,
                target: fixture.in_c,
                edge: Some(requested_edge),
            },
            NodeGraphConnectionMode::Strict,
            &NodeGraphInteractionState::default(),
        )
        .expect("connect should plan")
        .expect("connect should create transaction");

        match transaction.ops() {
            [GraphOp::AddEdge { id, edge }] => {
                assert_eq!(*id, requested_edge);
                assert_eq!(edge.from, fixture.out_a);
                assert_eq!(edge.to, fixture.in_c);
            }
            ops => panic!("expected one AddEdge op, got {ops:?}"),
        }
    }

    #[test]
    fn connection_sync_reconnect_preserves_edge_id() {
        let fixture = connection_fixture();
        let transaction = plan_connection_sync_transaction(
            &fixture.graph,
            OpenGpuiConnectionSyncRequest::Reconnect {
                edge: fixture.edge_ab,
                endpoint: EdgeEndpoint::To,
                new_port: fixture.in_c,
            },
            NodeGraphConnectionMode::Strict,
            &NodeGraphInteractionState::default(),
        )
        .expect("reconnect should plan")
        .expect("reconnect should create transaction");

        match transaction.ops() {
            [GraphOp::SetEdgeEndpoints { id, to, .. }] => {
                assert_eq!(*id, fixture.edge_ab);
                assert_eq!(to.from, fixture.out_a);
                assert_eq!(to.to, fixture.in_c);
            }
            ops => panic!("expected one SetEdgeEndpoints op, got {ops:?}"),
        }
    }

    #[test]
    fn connection_sync_delete_uses_runtime_delete_plan() {
        let fixture = connection_fixture();
        let transaction = plan_connection_sync_transaction(
            &fixture.graph,
            OpenGpuiConnectionSyncRequest::Delete {
                edge: fixture.edge_ab,
            },
            NodeGraphConnectionMode::Strict,
            &NodeGraphInteractionState::default(),
        )
        .expect("delete should plan")
        .expect("delete should create transaction");

        assert!(transaction.ops().iter().any(|op| matches!(
            op,
            GraphOp::RemoveEdge { id, .. } if *id == fixture.edge_ab
        )));
    }

    struct ConnectionFixture {
        graph: Graph,
        out_a: PortId,
        in_c: PortId,
        edge_ab: EdgeId,
    }

    fn connection_fixture() -> ConnectionFixture {
        let a = NodeId::from_u128(1);
        let b = NodeId::from_u128(2);
        let c = NodeId::from_u128(3);
        let out_a = PortId::from_u128(10);
        let in_b = PortId::from_u128(20);
        let in_c = PortId::from_u128(30);
        let edge_ab = EdgeId::from_u128(100);
        let mut graph = Graph::new(GraphId::from_u128(1));
        GraphTransaction::from_ops([
            GraphOp::AddNode {
                id: a,
                node: node_with_ports([out_a]),
            },
            GraphOp::AddNode {
                id: b,
                node: node_with_ports([in_b]),
            },
            GraphOp::AddNode {
                id: c,
                node: node_with_ports([in_c]),
            },
            GraphOp::AddPort {
                id: out_a,
                port: port(a, "out", jellyflow::core::PortDirection::Out),
            },
            GraphOp::AddPort {
                id: in_b,
                port: port(b, "in", jellyflow::core::PortDirection::In),
            },
            GraphOp::AddPort {
                id: in_c,
                port: port(c, "in", jellyflow::core::PortDirection::In),
            },
            GraphOp::AddEdge {
                id: edge_ab,
                edge: Edge::new(EdgeKind::Data, out_a, in_b),
            },
        ])
        .apply_to(&mut graph)
        .expect("fixture graph should apply");
        ConnectionFixture {
            graph,
            out_a,
            in_c,
            edge_ab,
        }
    }

    fn node_with_ports(ports: impl IntoIterator<Item = PortId>) -> Node {
        Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: Default::default(),
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: ports.into_iter().collect(),
            data: serde_json::Value::Null,
        }
    }

    fn port(node: NodeId, key: &str, dir: jellyflow::core::PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }
}
