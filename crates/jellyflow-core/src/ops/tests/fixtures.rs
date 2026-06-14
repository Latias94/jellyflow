use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ConnectedPairIds {
    pub a: NodeId,
    pub b: NodeId,
    pub out: PortId,
    pub inn: PortId,
    pub edge: EdgeId,
}

impl ConnectedPairIds {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

impl Default for ConnectedPairIds {
    fn default() -> Self {
        Self {
            a: NodeId::new(),
            b: NodeId::new(),
            out: PortId::new(),
            inn: PortId::new(),
            edge: EdgeId::new(),
        }
    }
}

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

pub(super) fn make_edge(from: PortId, to: PortId) -> Edge {
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
    }
}

pub(super) fn make_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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

pub(super) fn insert_node(graph: &mut Graph, id: NodeId, kind: &str) {
    graph.insert_node(id, make_node(kind));
}

pub(super) fn insert_port(
    graph: &mut Graph,
    id: PortId,
    node: NodeId,
    key: &str,
    dir: PortDirection,
) {
    graph.insert_port(id, make_port(node, key, dir));
    graph.node_mut(&node).unwrap().ports.push(id);
}

pub(super) fn insert_edge(graph: &mut Graph, id: EdgeId, from: PortId, to: PortId) {
    graph.insert_edge(id, make_edge(from, to));
}

pub(super) fn insert_connected_pair(graph: &mut Graph) -> ConnectedPairIds {
    insert_connected_pair_with_ids(graph, ConnectedPairIds::new())
}

pub(super) fn insert_connected_pair_with_ids(
    graph: &mut Graph,
    ids: ConnectedPairIds,
) -> ConnectedPairIds {
    insert_node(graph, ids.a, "core.a");
    insert_node(graph, ids.b, "core.b");
    insert_port(graph, ids.out, ids.a, "out", PortDirection::Out);
    insert_port(graph, ids.inn, ids.b, "in", PortDirection::In);
    insert_edge(graph, ids.edge, ids.out, ids.inn);
    ids
}
