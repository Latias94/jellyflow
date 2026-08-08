use super::*;
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeKind, EdgeReconnectable, EdgeReconnectableEndpoint, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

fn node() -> Node {
    Node {
        kind: NodeKindKey::new("test"),
        kind_version: 1,
        pos: CanvasPoint::default(),
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn port(node: NodeId) -> Port {
    Port {
        node,
        key: PortKey::new("out"),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn edge(from: PortId, to: PortId) -> Edge {
    Edge::new(EdgeKind::Data, from, to)
}

#[test]
fn default_policy_allows_authoritative_graph_mutations() {
    let policy = GraphRuntimePolicy::default();

    assert!(policy.node_connections_enabled());
    assert!(policy.node_deletion_enabled());
    assert!(policy.edge_deletion_enabled());
    assert!(policy.edge_reconnection_enabled());
    assert!(policy.cycles_allowed(EdgeKind::Data));
    assert!(policy.cycles_allowed(EdgeKind::Exec));
}

#[test]
fn cycle_policy_is_resolved_independently_per_edge_kind() {
    let policy = GraphRuntimePolicy::default().with_cycles(EdgeKind::Data, false);

    assert!(!policy.cycles_allowed(EdgeKind::Data));
    assert!(policy.cycles_allowed(EdgeKind::Exec));
}

#[test]
fn persisted_element_overrides_take_precedence_over_runtime_fallbacks() {
    let disabled = GraphRuntimePolicy::default()
        .with_node_connections(false)
        .with_node_deletion(false)
        .with_edge_deletion(false)
        .with_edge_reconnection(false);

    let default_node = resolve_node_policy(&node(), &disabled);
    assert!(!default_node.connectable);
    assert!(!default_node.can_delete());

    let mut enabled_node = node();
    enabled_node.connectable = Some(true);
    enabled_node.deletable = Some(true);
    let enabled_node_policy = resolve_node_policy(&enabled_node, &disabled);
    assert!(enabled_node_policy.connectable);
    assert!(enabled_node_policy.can_delete());

    let from = PortId::new();
    let to = PortId::new();
    let mut enabled_edge = edge(from, to);
    enabled_edge.deletable = Some(true);
    enabled_edge.reconnectable = Some(EdgeReconnectable::Endpoint(
        EdgeReconnectableEndpoint::Source,
    ));
    let enabled_edge_policy = resolve_edge_policy(&enabled_edge, &disabled);
    assert!(enabled_edge_policy.can_select());
    assert!(enabled_edge_policy.can_delete());
    assert!(enabled_edge_policy.reconnectable());
    assert!(enabled_edge_policy.can_reconnect_source());
    assert!(!enabled_edge_policy.can_reconnect_target());

    enabled_edge.selectable = Some(false);
    assert!(!resolve_edge_policy(&enabled_edge, &disabled).can_select());
}

#[test]
fn port_overrides_can_only_restrict_the_resolved_node_connection_policy() {
    let node_id = NodeId::new();
    let mut current_node = node();
    current_node.connectable = Some(true);
    let mut current_port = port(node_id);
    current_port.connectable_start = Some(false);
    current_port.connectable_end = Some(true);

    let policy = resolve_port_policy(&current_node, &current_port, &GraphRuntimePolicy::default());
    assert!(policy.connectable);
    assert!(!policy.can_start_connection());
    assert!(policy.can_accept_connection());

    current_node.connectable = Some(false);
    current_port.connectable_start = Some(true);
    let disabled =
        resolve_port_policy(&current_node, &current_port, &GraphRuntimePolicy::default());
    assert!(!disabled.connectable);
    assert!(!disabled.can_start_connection());
    assert!(!disabled.can_accept_connection());
}
