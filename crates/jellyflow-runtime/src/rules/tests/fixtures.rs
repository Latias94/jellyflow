use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, Node, NodeId, NodeKindKey, Port, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};

pub(super) fn make_node(kind: &str) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 0,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
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

pub(super) fn insert_port(graph: &mut Graph, id: PortId, port: Port) {
    let node = port.node;
    graph.ports.insert(id, port);
    graph
        .nodes
        .get_mut(&node)
        .expect("test port owner must exist")
        .ports
        .push(id);
}

pub(super) fn insert_edge(graph: &mut Graph, id: EdgeId, from: PortId, to: PortId) {
    graph.edges.insert(
        id,
        Edge {
            kind: EdgeKind::Data,
            from,
            to,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );
}
