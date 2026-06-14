use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, Group, GroupId,
    Node, NodeId, NodeKindKey, Port, PortId, StickyNote, StickyNoteId,
};

use crate::io::NodeGraphViewState;
use crate::runtime::store::NodeGraphStore;

pub(super) fn default_editor_config() -> crate::io::NodeGraphEditorConfig {
    crate::io::NodeGraphEditorConfig::default()
}

pub(super) fn make_store(graph: Graph) -> NodeGraphStore {
    NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        default_editor_config(),
    )
}

pub(super) fn fixture_insert_node(graph: &mut Graph, id: NodeId, node: Node) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.insert_node(id, node);
    *graph = builder.build_unchecked();
}

pub(super) fn fixture_insert_port(graph: &mut Graph, id: PortId, port: Port) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.insert_port(id, port);
    *graph = builder.build_unchecked();
}

pub(super) fn fixture_insert_group(graph: &mut Graph, id: GroupId, group: Group) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.insert_group(id, group);
    *graph = builder.build_unchecked();
}

pub(super) fn fixture_insert_sticky_note(graph: &mut Graph, id: StickyNoteId, note: StickyNote) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.insert_sticky_note(id, note);
    *graph = builder.build_unchecked();
}

pub(super) fn fixture_insert_binding(graph: &mut Graph, id: BindingId, binding: Binding) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.insert_binding(id, binding);
    *graph = builder.build_unchecked();
}

pub(super) fn fixture_remove_edge(graph: &mut Graph, id: &EdgeId) -> Option<Edge> {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    let removed = builder.remove_edge(id);
    *graph = builder.build_unchecked();
    removed
}

pub(super) fn fixture_clear_edges(graph: &mut Graph) {
    let mut builder = GraphBuilder::from_graph(std::mem::take(graph));
    builder.clear_edges();
    *graph = builder.build_unchecked();
}

pub(super) fn make_graph() -> (
    Graph,
    NodeId,
    NodeId,
    jellyflow_core::core::PortId,
    jellyflow_core::core::PortId,
    EdgeId,
) {
    let mut g = GraphBuilder::new(jellyflow_core::core::GraphId::from_u128(1));

    let a = NodeId::new();
    let b = NodeId::new();

    let out_port = jellyflow_core::core::PortId::new();
    let in_port = jellyflow_core::core::PortId::new();

    let node_a = Node {
        kind: NodeKindKey::new("demo.a"),
        kind_version: 1,
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
        ports: vec![out_port],
        data: serde_json::Value::Null,
    };
    let node_b = Node {
        kind: NodeKindKey::new("demo.b"),
        kind_version: 1,
        pos: CanvasPoint { x: 100.0, y: 0.0 },
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
        ports: vec![in_port],
        data: serde_json::Value::Null,
    };

    g.insert_node(a, node_a);
    g.insert_node(b, node_b);
    g.insert_port(
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
    g.insert_port(
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
    g.insert_edge(
        eid,
        Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
    );

    (g.into(), a, b, out_port, in_port, eid)
}
