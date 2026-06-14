use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, GraphBuilder, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::types::TypeDesc;

pub(super) fn make_node(kind: &str) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 0,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
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
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

pub(super) fn insert_node(graph: &mut GraphBuilder, id: NodeId, kind: &str) {
    graph.insert_node(id, make_node(kind));
}

pub(super) fn make_port(
    node: NodeId,
    key: &str,
    dir: PortDirection,
    kind: PortKind,
    capacity: PortCapacity,
) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind,
        capacity,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

pub(super) fn make_data_port(
    node: NodeId,
    key: &str,
    dir: PortDirection,
    capacity: PortCapacity,
) -> Port {
    make_port(node, key, dir, PortKind::Data, capacity)
}

pub(super) fn make_data_output(node: NodeId, key: &str, capacity: PortCapacity) -> Port {
    make_data_port(node, key, PortDirection::Out, capacity)
}

pub(super) fn make_data_input(node: NodeId, key: &str, capacity: PortCapacity) -> Port {
    make_data_port(node, key, PortDirection::In, capacity)
}

pub(super) fn insert_port(graph: &mut GraphBuilder, id: PortId, port: Port) {
    let node = port.node;
    graph.insert_port(id, port);
    graph
        .update_node(&node, |node| node.ports.push(id))
        .expect("test port owner must exist");
}

pub(super) fn insert_data_port(
    graph: &mut GraphBuilder,
    id: PortId,
    node: NodeId,
    key: &str,
    dir: PortDirection,
    capacity: PortCapacity,
) {
    insert_port(graph, id, make_data_port(node, key, dir, capacity));
}

pub(super) fn insert_data_output(
    graph: &mut GraphBuilder,
    id: PortId,
    node: NodeId,
    key: &str,
    capacity: PortCapacity,
) {
    insert_data_port(graph, id, node, key, PortDirection::Out, capacity);
}

pub(super) fn insert_data_input(
    graph: &mut GraphBuilder,
    id: PortId,
    node: NodeId,
    key: &str,
    capacity: PortCapacity,
) {
    insert_data_port(graph, id, node, key, PortDirection::In, capacity);
}

pub(super) fn insert_typed_data_output(
    graph: &mut GraphBuilder,
    id: PortId,
    node: NodeId,
    key: &str,
    capacity: PortCapacity,
    ty: TypeDesc,
) {
    let mut port = make_data_output(node, key, capacity);
    port.ty = Some(ty);
    insert_port(graph, id, port);
}

pub(super) fn insert_typed_data_input(
    graph: &mut GraphBuilder,
    id: PortId,
    node: NodeId,
    key: &str,
    capacity: PortCapacity,
    ty: TypeDesc,
) {
    let mut port = make_data_input(node, key, capacity);
    port.ty = Some(ty);
    insert_port(graph, id, port);
}

pub(super) fn insert_edge(graph: &mut GraphBuilder, id: EdgeId, from: PortId, to: PortId) {
    graph.insert_edge(
        id,
        Edge {
            kind: EdgeKind::Data,
            from,
            to,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
    );
}
