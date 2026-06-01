use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};

pub(super) fn node_at(pos: CanvasPoint, size: Option<CanvasSize>) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

pub(super) fn out_port(node: NodeId) -> (PortId, Port) {
    let pid = PortId::new();
    (
        pid,
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
        },
    )
}

pub(super) fn in_port(node: NodeId, key: &str) -> (PortId, Port) {
    let pid = PortId::new();
    (
        pid,
        Port {
            node,
            key: PortKey::new(key),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    )
}
