//! Effective editor policy resolution for graph elements.
//!
//! `jellyflow-core` stores per-element policy overrides because they are part of the persisted
//! graph document. Runtime adapters should resolve those overrides through this module instead of
//! duplicating precedence rules.

use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{
    Edge, EdgeReconnectable, EdgeReconnectableEndpoint, Node, NodeExtent, Port,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphNodeInteractionPolicy {
    pub selectable: bool,
    pub draggable: bool,
    pub connectable: bool,
    pub deletable: bool,
    pub extent: Option<NodeExtent>,
    pub expand_parent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphPortInteractionPolicy {
    pub connectable: bool,
    pub connectable_start: bool,
    pub connectable_end: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphEdgeInteractionPolicy {
    pub selectable: bool,
    pub deletable: bool,
    pub reconnect_source: bool,
    pub reconnect_target: bool,
}

impl NodeGraphEdgeInteractionPolicy {
    pub fn reconnectable(self) -> bool {
        self.reconnect_source || self.reconnect_target
    }
}

pub fn resolve_node_interaction_policy(
    node: &Node,
    state: &NodeGraphInteractionState,
) -> NodeGraphNodeInteractionPolicy {
    NodeGraphNodeInteractionPolicy {
        selectable: node.selectable.unwrap_or(state.elements_selectable),
        draggable: node.draggable.unwrap_or(state.nodes_draggable),
        connectable: node.connectable.unwrap_or(state.nodes_connectable),
        deletable: node.deletable.unwrap_or(state.nodes_deletable),
        extent: node
            .extent
            .or_else(|| state.node_extent.map(|rect| NodeExtent::Rect { rect })),
        expand_parent: node.expand_parent.unwrap_or(false),
    }
}

pub fn resolve_port_interaction_policy(
    node: &Node,
    port: &Port,
    state: &NodeGraphInteractionState,
) -> NodeGraphPortInteractionPolicy {
    let node_policy = resolve_node_interaction_policy(node, state);
    let connectable = node_policy.connectable && port.connectable.unwrap_or(true);
    NodeGraphPortInteractionPolicy {
        connectable,
        connectable_start: connectable && port.connectable_start.unwrap_or(true),
        connectable_end: connectable && port.connectable_end.unwrap_or(true),
    }
}

pub fn resolve_edge_interaction_policy(
    edge: &Edge,
    state: &NodeGraphInteractionState,
) -> NodeGraphEdgeInteractionPolicy {
    let reconnectable = edge
        .reconnectable
        .unwrap_or(EdgeReconnectable::Bool(state.edges_reconnectable));
    let (reconnect_source, reconnect_target) = match reconnectable {
        EdgeReconnectable::Bool(enabled) => (enabled, enabled),
        EdgeReconnectable::Endpoint(EdgeReconnectableEndpoint::Source) => (true, false),
        EdgeReconnectable::Endpoint(EdgeReconnectableEndpoint::Target) => (false, true),
    };

    NodeGraphEdgeInteractionPolicy {
        selectable: edge.selectable.unwrap_or(state.edges_selectable),
        deletable: edge.deletable.unwrap_or(state.edges_deletable),
        reconnect_source,
        reconnect_target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_core::core::{
        CanvasPoint, CanvasRect, CanvasSize, EdgeKind, NodeId, NodeKindKey, PortCapacity,
        PortDirection, PortId, PortKey, PortKind,
    };

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
        assert!(!default_policy.reconnectable());

        let mut e = edge(from, to);
        e.selectable = Some(true);
        e.deletable = Some(true);
        e.reconnectable = Some(EdgeReconnectable::Bool(true));
        let enabled_policy = resolve_edge_interaction_policy(&e, &disabled_state);
        assert!(enabled_policy.selectable);
        assert!(enabled_policy.deletable);
        assert!(enabled_policy.reconnect_source);
        assert!(enabled_policy.reconnect_target);
        assert!(enabled_policy.reconnectable());

        e.reconnectable = Some(EdgeReconnectable::Endpoint(
            EdgeReconnectableEndpoint::Source,
        ));
        let source_only = resolve_edge_interaction_policy(&e, &disabled_state);
        assert!(source_only.reconnect_source);
        assert!(!source_only.reconnect_target);

        e.reconnectable = Some(EdgeReconnectable::Endpoint(
            EdgeReconnectableEndpoint::Target,
        ));
        let target_only = resolve_edge_interaction_policy(&e, &disabled_state);
        assert!(!target_only.reconnect_source);
        assert!(target_only.reconnect_target);
    }
}
