use crate::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, Group, GroupId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use crate::ops::fragment::GraphFragment;
use serde_json::Value;

fn node(kind: &str, pos: (f32, f32), parent: Option<GroupId>, ports: &[PortId]) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x: pos.0, y: pos.1 },
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: ports.to_vec(),
        data: Value::Null,
    }
}

fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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
        data: Value::Null,
    }
}

#[test]
fn from_selection_detaches_nodes_when_group_is_not_selected() {
    let mut graph = Graph::default();

    let group_id = GroupId::new();
    graph.insert_group(
        group_id,
        Group {
            title: "G".to_string(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );

    let node_id = NodeId::new();
    let in_port = PortId::new();
    graph.insert_node(node_id, node("A", (1.0, 2.0), Some(group_id), &[in_port]));
    graph.insert_port(in_port, port(node_id, "in", PortDirection::In));

    let fragment = GraphFragment::from_selection(&graph, [node_id], std::iter::empty());
    assert!(fragment.groups.is_empty());
    assert_eq!(fragment.nodes.len(), 1);
    assert_eq!(fragment.nodes[&node_id].parent, None);
}

#[test]
fn from_selection_keeps_parent_when_group_is_selected_and_includes_children() {
    let mut graph = Graph::default();

    let group_id = GroupId::new();
    graph.insert_group(
        group_id,
        Group {
            title: "G".to_string(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );

    let a = NodeId::new();
    let a_out = PortId::new();
    graph.insert_node(a, node("A", (1.0, 2.0), Some(group_id), &[a_out]));
    graph.insert_port(a_out, port(a, "out", PortDirection::Out));

    let b = NodeId::new();
    let b_in = PortId::new();
    graph.insert_node(b, node("B", (3.0, 4.0), Some(group_id), &[b_in]));
    graph.insert_port(b_in, port(b, "in", PortDirection::In));

    let e = EdgeId::new();
    graph.insert_edge(e, Edge::new(EdgeKind::Data, a_out, b_in));

    let fragment = GraphFragment::from_selection(&graph, std::iter::empty(), [group_id]);
    assert_eq!(fragment.groups.len(), 1);
    assert!(fragment.nodes.contains_key(&a));
    assert!(fragment.nodes.contains_key(&b));
    assert_eq!(fragment.nodes[&a].parent, Some(group_id));
    assert_eq!(fragment.nodes[&b].parent, Some(group_id));
    assert_eq!(fragment.edges.len(), 1);
}
