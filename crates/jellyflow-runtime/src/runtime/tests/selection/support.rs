use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

pub(super) struct SelectionFixture {
    pub(super) graph: Graph,
    pub(super) low: NodeId,
    pub(super) high: NodeId,
    pub(super) outside: NodeId,
    pub(super) connected_edge: EdgeId,
    pub(super) connected_outside_edge: EdgeId,
    pub(super) non_selectable_edge: EdgeId,
}

pub(super) fn selection_fixture() -> SelectionFixture {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let low = NodeId::from_u128(10);
    let hidden = NodeId::from_u128(20);
    let high = NodeId::from_u128(30);
    let disabled = NodeId::from_u128(40);
    let outside = NodeId::from_u128(50);

    let (low_out, low_port) = out_port(low, 100);
    let (high_in, high_port) = in_port(high, 101);
    let (outside_in, outside_port) = in_port(outside, 102);

    graph.insert_node(
        high,
        node(
            CanvasPoint { x: 12.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![high_in],
        ),
    );
    graph.insert_node(
        hidden,
        node(
            CanvasPoint { x: 24.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            true,
            Vec::new(),
        ),
    );
    graph.insert_node(
        low,
        node(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![low_out],
        ),
    );
    graph.insert_node(
        disabled,
        node(
            CanvasPoint { x: 36.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            Some(false),
            false,
            Vec::new(),
        ),
    );
    graph.insert_node(
        outside,
        node(
            CanvasPoint { x: 80.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![outside_in],
        ),
    );
    graph.insert_port(low_out, low_port);
    graph.insert_port(high_in, high_port);
    graph.insert_port(outside_in, outside_port);

    let connected_edge = EdgeId::from_u128(200);
    graph.insert_edge(connected_edge, Edge::new(EdgeKind::Data, low_out, high_in));

    let non_selectable_edge = EdgeId::from_u128(201);
    graph.insert_edge(
        non_selectable_edge,
        Edge {
            selectable: Some(false),
            ..Edge::new(EdgeKind::Data, low_out, outside_in)
        },
    );

    let connected_outside_edge = EdgeId::from_u128(202);
    graph.insert_edge(
        connected_outside_edge,
        Edge::new(EdgeKind::Data, low_out, outside_in),
    );

    SelectionFixture {
        graph: graph.into(),
        low,
        high,
        outside,
        connected_edge,
        connected_outside_edge,
        non_selectable_edge,
    }
}

pub(super) fn selection_rect() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint { x: -1.0, y: -1.0 },
        size: CanvasSize {
            width: 50.0,
            height: 20.0,
        },
    }
}

fn node(
    pos: CanvasPoint,
    size: Option<CanvasSize>,
    selectable: Option<bool>,
    hidden: bool,
    ports: Vec<PortId>,
) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn out_port(node: NodeId, stable: u128) -> (PortId, Port) {
    port(node, stable, PortDirection::Out, PortCapacity::Multi)
}

fn in_port(node: NodeId, stable: u128) -> (PortId, Port) {
    port(node, stable, PortDirection::In, PortCapacity::Single)
}

fn port(node: NodeId, stable: u128, dir: PortDirection, capacity: PortCapacity) -> (PortId, Port) {
    let id = PortId::from_u128(stable);
    (
        id,
        Port {
            node,
            key: PortKey::new(format!("p{stable}")),
            dir,
            kind: PortKind::Data,
            capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    )
}
