use jellyflow_core::{
    CanvasPoint, CanvasSize, Graph, GraphId, GraphOp, GraphTransaction, Node, NodeId, NodeKindKey,
};

fn make_node(kind: &str, x: f32, y: f32) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x, y },
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 80.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::json!({
            "label": kind,
        }),
    }
}

fn main() {
    let node_id = NodeId::from_u128(2);
    let mut graph = Graph::new(GraphId::from_u128(1));

    let mut tx = GraphTransaction::new().with_label("add source node");
    tx.push(GraphOp::AddNode {
        id: node_id,
        node: make_node("demo.source", 10.0, 20.0),
    });
    tx.apply_to(&mut graph).expect("transaction applies");

    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[&node_id].kind, NodeKindKey::new("demo.source"));
}
