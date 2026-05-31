use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, Node, NodeId, NodeKindKey, Port,
};

pub(super) fn default_editor_config() -> crate::io::NodeGraphEditorConfig {
    crate::io::NodeGraphEditorConfig::default()
}

pub(super) fn make_graph() -> (
    Graph,
    NodeId,
    NodeId,
    jellyflow_core::core::PortId,
    jellyflow_core::core::PortId,
    EdgeId,
) {
    let mut g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));

    let a = NodeId::new();
    let b = NodeId::new();

    let out_port = jellyflow_core::core::PortId::new();
    let in_port = jellyflow_core::core::PortId::new();

    let node_a = Node {
        kind: NodeKindKey::new("demo.a"),
        kind_version: 1,
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
        ports: vec![out_port],
        data: serde_json::Value::Null,
    };
    let node_b = Node {
        kind: NodeKindKey::new("demo.b"),
        kind_version: 1,
        pos: CanvasPoint { x: 100.0, y: 0.0 },
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
        ports: vec![in_port],
        data: serde_json::Value::Null,
    };

    g.nodes.insert(a, node_a);
    g.nodes.insert(b, node_b);
    g.ports.insert(
        out_port,
        Port {
            node: a,
            key: jellyflow_core::core::PortKey::new("out"),
            dir: jellyflow_core::core::PortDirection::Out,
            kind: jellyflow_core::core::PortKind::Data,
            capacity: jellyflow_core::core::PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    g.ports.insert(
        in_port,
        Port {
            node: b,
            key: jellyflow_core::core::PortKey::new("in"),
            dir: jellyflow_core::core::PortDirection::In,
            kind: jellyflow_core::core::PortKind::Data,
            capacity: jellyflow_core::core::PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );

    let eid = EdgeId::new();
    g.edges.insert(
        eid,
        Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    (g, a, b, out_port, in_port, eid)
}
