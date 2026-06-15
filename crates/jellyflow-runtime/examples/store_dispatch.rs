use jellyflow_core::{
    CanvasPoint, CanvasSize, Graph, GraphId, GraphOp, GraphTransaction, Node, NodeId, NodeKindKey,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::xyflow::NodeGraphChanges;

fn make_node(kind: &str, x: f32, y: f32) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x, y },
        origin: None,
        selectable: None,
        focusable: None,
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
    let node = make_node("demo.source", 10.0, 20.0);
    let mut graph = Graph::new(GraphId::from_u128(1));

    let mut add_node = GraphTransaction::new().with_label("add source node");
    add_node.push(GraphOp::AddNode { id: node_id, node });
    add_node.apply_to(&mut graph).expect("transaction applies");

    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    let mut move_node = GraphTransaction::new().with_label("move source node");
    move_node.push(GraphOp::SetNodePos {
        id: node_id,
        from: CanvasPoint { x: 10.0, y: 20.0 },
        to: CanvasPoint { x: 32.0, y: 48.0 },
    });

    let outcome = store
        .dispatch_transaction(&move_node)
        .expect("store dispatch succeeds");
    let changes = NodeGraphChanges::from_patch(outcome.patch());

    assert_eq!(outcome.committed().ops().len(), 1);
    assert_eq!(changes.nodes.len(), 1);
    assert!(changes.edges.is_empty());
    assert_eq!(
        store.graph().nodes()[&node_id].pos,
        CanvasPoint { x: 32.0, y: 48.0 }
    );

    println!(
        "committed {} op(s), projected {} node change(s) and {} edge change(s)",
        outcome.committed().ops().len(),
        changes.nodes.len(),
        changes.edges.len()
    );
}
