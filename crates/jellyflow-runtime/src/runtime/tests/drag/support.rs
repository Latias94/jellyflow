use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, Group, GroupId, Node, NodeId, NodeKindKey,
};

pub(super) struct DragFixture {
    pub(super) graph: Graph,
    pub(super) enabled: NodeId,
    pub(super) disabled: NodeId,
    pub(super) hidden: NodeId,
    pub(super) missing: NodeId,
    pub(super) selected_low: NodeId,
    pub(super) selected_high: NodeId,
    pub(super) child_in_selected_group: NodeId,
    pub(super) selected_group: GroupId,
}

pub(super) fn drag_fixture() -> DragFixture {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let selected_low = NodeId::from_u128(5);
    let enabled = NodeId::from_u128(10);
    let disabled = NodeId::from_u128(20);
    let hidden = NodeId::from_u128(30);
    let missing = NodeId::from_u128(40);
    let selected_high = NodeId::from_u128(60);
    let child_in_selected_group = NodeId::from_u128(70);
    let selected_group = GroupId::from_u128(100);

    graph.nodes.insert(
        selected_low,
        node(CanvasPoint { x: 0.0, y: 0.0 }, None, false),
    );
    graph
        .nodes
        .insert(enabled, node(CanvasPoint { x: 10.0, y: 20.0 }, None, false));
    graph.nodes.insert(
        disabled,
        node(CanvasPoint { x: 200.0, y: 0.0 }, Some(false), false),
    );
    graph
        .nodes
        .insert(hidden, node(CanvasPoint { x: 10.0, y: 20.0 }, None, true));
    graph.nodes.insert(
        selected_high,
        node(CanvasPoint { x: 100.0, y: 0.0 }, None, false),
    );
    graph.groups.insert(
        selected_group,
        Group {
            title: "Selected Group".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 280.0, y: -20.0 },
                size: CanvasSize {
                    width: 80.0,
                    height: 80.0,
                },
            },
            color: None,
        },
    );
    graph.nodes.insert(
        child_in_selected_group,
        node_with_parent(
            CanvasPoint { x: 300.0, y: 0.0 },
            None,
            false,
            selected_group,
        ),
    );

    DragFixture {
        graph,
        enabled,
        disabled,
        hidden,
        missing,
        selected_low,
        selected_high,
        child_in_selected_group,
        selected_group,
    }
}

fn node_with_parent(
    pos: CanvasPoint,
    draggable: Option<bool>,
    hidden: bool,
    parent: GroupId,
) -> Node {
    let mut node = node(pos, draggable, hidden);
    node.parent = Some(parent);
    node
}

fn node(pos: CanvasPoint, draggable: Option<bool>, hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        selectable: None,
        draggable,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}
