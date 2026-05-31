use super::*;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeKind, EdgeReconnectable,
    EdgeReconnectableEndpoint, Node, NodeExtent, NodeId, NodeKindKey, Port, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};

use crate::io::NodeGraphInteractionState;

fn node() -> Node {
    Node {
        kind: NodeKindKey::new("test"),
        kind_version: 1,
        pos: CanvasPoint::default(),
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
    Edge {
        kind: EdgeKind::Data,
        from,
        to,
        selectable: None,
        deletable: None,
        reconnectable: None,
    }
}

#[test]
fn policy_node_overrides_global_defaults() {
    let mut state = NodeGraphInteractionState {
        elements_selectable: false,
        nodes_draggable: false,
        nodes_connectable: false,
        nodes_deletable: false,
        node_extent: Some(CanvasRect {
            origin: CanvasPoint { x: 1.0, y: 2.0 },
            size: CanvasSize {
                width: 10.0,
                height: 20.0,
            },
        }),
        ..NodeGraphInteractionState::default()
    };

    let default_policy = resolve_node_interaction_policy(&node(), &state);
    assert_eq!(
        default_policy,
        NodeGraphNodeInteractionPolicy {
            selectable: false,
            draggable: false,
            connectable: false,
            deletable: false,
            extent: state.node_extent.map(|rect| NodeExtent::Rect { rect }),
            expand_parent: false,
        }
    );
    assert!(!default_policy.can_delete());

    state.node_extent = None;
    let mut n = node();
    n.selectable = Some(true);
    n.draggable = Some(true);
    n.connectable = Some(true);
    n.deletable = Some(true);
    n.extent = Some(NodeExtent::Parent);
    n.expand_parent = Some(true);

    assert_eq!(
        resolve_node_interaction_policy(&n, &state),
        NodeGraphNodeInteractionPolicy {
            selectable: true,
            draggable: true,
            connectable: true,
            deletable: true,
            extent: Some(NodeExtent::Parent),
            expand_parent: true,
        }
    );
    assert!(resolve_node_interaction_policy(&n, &state).can_delete());
}

#[test]
fn policy_port_requires_node_and_port_connectability() {
    let node_id = NodeId::new();
    let state = NodeGraphInteractionState {
        nodes_connectable: true,
        ..NodeGraphInteractionState::default()
    };

    let mut n = node();
    n.connectable = Some(false);
    let mut p = port(node_id);
    p.connectable = Some(true);
    p.connectable_start = Some(true);
    p.connectable_end = Some(true);
    assert_eq!(
        resolve_port_interaction_policy(&n, &p, &state),
        NodeGraphPortInteractionPolicy {
            connectable: false,
            connectable_start: false,
            connectable_end: false,
        }
    );
    assert!(!resolve_port_interaction_policy(&n, &p, &state).can_start_connection());

    n.connectable = Some(true);
    p.connectable = Some(false);
    assert!(!resolve_port_interaction_policy(&n, &p, &state).connectable);

    p.connectable = None;
    p.connectable_start = Some(false);
    p.connectable_end = Some(true);
    assert_eq!(
        resolve_port_interaction_policy(&n, &p, &state),
        NodeGraphPortInteractionPolicy {
            connectable: true,
            connectable_start: false,
            connectable_end: true,
        }
    );
    let policy = resolve_port_interaction_policy(&n, &p, &state);
    assert!(!policy.can_start_connection());
    assert!(policy.can_accept_connection());
}

#[test]
fn policy_edge_overrides_global_defaults_and_preserves_endpoint_reconnectability() {
    let from = PortId::new();
    let to = PortId::new();
    let disabled_state = NodeGraphInteractionState {
        edges_selectable: false,
        edges_deletable: false,
        edges_reconnectable: false,
        ..NodeGraphInteractionState::default()
    };

    let default_policy = resolve_edge_interaction_policy(&edge(from, to), &disabled_state);
    assert_eq!(
        default_policy,
        NodeGraphEdgeInteractionPolicy {
            selectable: false,
            deletable: false,
            reconnect_source: false,
            reconnect_target: false,
        }
    );
    assert!(!default_policy.can_delete());
    assert!(!default_policy.reconnectable());
    assert!(!default_policy.can_reconnect_source());
    assert!(!default_policy.can_reconnect_target());

    let mut e = edge(from, to);
    e.selectable = Some(true);
    e.deletable = Some(true);
    e.reconnectable = Some(EdgeReconnectable::Bool(true));
    let enabled_policy = resolve_edge_interaction_policy(&e, &disabled_state);
    assert!(enabled_policy.selectable);
    assert!(enabled_policy.deletable);
    assert!(enabled_policy.can_delete());
    assert!(enabled_policy.reconnect_source);
    assert!(enabled_policy.reconnect_target);
    assert!(enabled_policy.reconnectable());

    e.reconnectable = Some(EdgeReconnectable::Endpoint(
        EdgeReconnectableEndpoint::Source,
    ));
    let source_only = resolve_edge_interaction_policy(&e, &disabled_state);
    assert!(source_only.reconnect_source);
    assert!(!source_only.reconnect_target);
    assert!(source_only.can_reconnect_source());
    assert!(!source_only.can_reconnect_target());

    e.reconnectable = Some(EdgeReconnectable::Endpoint(
        EdgeReconnectableEndpoint::Target,
    ));
    let target_only = resolve_edge_interaction_policy(&e, &disabled_state);
    assert!(!target_only.reconnect_source);
    assert!(target_only.reconnect_target);
    assert!(!target_only.can_reconnect_source());
    assert!(target_only.can_reconnect_target());
}

#[test]
fn interaction_state_exposes_policy_facades() {
    let node_id = NodeId::new();
    let from = PortId::new();
    let to = PortId::new();
    let state = NodeGraphInteractionState::default();
    let n = node();
    let p = port(node_id);
    let e = edge(from, to);

    assert_eq!(
        state.node_interaction_policy(&n),
        resolve_node_interaction_policy(&n, &state)
    );
    assert_eq!(
        state.port_interaction_policy(&n, &p),
        resolve_port_interaction_policy(&n, &p, &state)
    );
    assert_eq!(
        state.edge_interaction_policy(&e),
        resolve_edge_interaction_policy(&e, &state)
    );
}
