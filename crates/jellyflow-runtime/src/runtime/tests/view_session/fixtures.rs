use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::ViewSessionSeed;
use crate::runtime::view_session::ViewSessionOptions;

pub(super) fn node(kind: &str, x: f32) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

pub(in crate::runtime::tests) fn graph_with_nodes(count: usize) -> (Graph, Vec<NodeId>) {
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x200));
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let id = NodeId::from_u128(index as u128 + 1);
        builder.insert_node(id, node("test.node", index as f32 * 24.0));
        ids.push(id);
    }
    (builder.build().expect("valid fixture graph"), ids)
}

#[derive(Clone, Copy)]
pub(in crate::runtime::tests) struct RoutedGraphIds {
    pub(in crate::runtime::tests) source: NodeId,
    pub(in crate::runtime::tests) target: NodeId,
    pub(in crate::runtime::tests) alternate_source: NodeId,
    pub(in crate::runtime::tests) source_port: PortId,
    pub(in crate::runtime::tests) target_port: PortId,
    pub(in crate::runtime::tests) alternate_source_port: PortId,
    pub(in crate::runtime::tests) edge: EdgeId,
}

pub(in crate::runtime::tests) fn graph_with_routed_edge() -> (Graph, RoutedGraphIds) {
    let ids = RoutedGraphIds {
        source: NodeId::from_u128(1),
        target: NodeId::from_u128(2),
        alternate_source: NodeId::from_u128(3),
        source_port: PortId::from_u128(11),
        target_port: PortId::from_u128(12),
        alternate_source_port: PortId::from_u128(13),
        edge: EdgeId::from_u128(21),
    };
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x201));
    for (id, port, direction, x) in [
        (ids.source, ids.source_port, PortDirection::Out, 0.0),
        (ids.target, ids.target_port, PortDirection::In, 240.0),
        (
            ids.alternate_source,
            ids.alternate_source_port,
            PortDirection::Out,
            480.0,
        ),
    ] {
        let mut current = node("test.node", x);
        current.ports = vec![port];
        builder.insert_node(id, current);
        builder.insert_port(port, port_for(id, direction));
    }
    builder.insert_edge(
        ids.edge,
        Edge::new(EdgeKind::Data, ids.source_port, ids.target_port),
    );
    (builder.build().expect("valid routed graph"), ids)
}

fn port_for(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(match direction {
            PortDirection::In => "in",
            PortDirection::Out => "out",
        }),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

pub(super) fn view_options(seed: ViewSessionSeed) -> ViewSessionOptions {
    ViewSessionOptions { seed: Some(seed) }
}
