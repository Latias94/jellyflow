use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, GraphOp,
    GraphTransaction, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::layout::{LayoutRequest, LayoutScope};

fn make_node(kind: &str, x: f32, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
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
            height: 72.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::json!({ "label": kind }),
    }
}

fn make_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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

fn make_graph() -> (Graph, NodeId, NodeId, NodeId, PortId, PortId) {
    let source = NodeId::from_u128(2);
    let target = NodeId::from_u128(3);
    let unrelated = NodeId::from_u128(4);
    let source_port = PortId::from_u128(10);
    let target_port = PortId::from_u128(11);

    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, make_node("demo.source", 0.0, vec![source_port]));
    graph.insert_node(target, make_node("demo.target", 320.0, vec![target_port]));
    graph.insert_node(unrelated, make_node("demo.unrelated", 640.0, Vec::new()));
    graph.insert_port(source_port, make_port(source, "out", PortDirection::Out));
    graph.insert_port(target_port, make_port(target, "in", PortDirection::In));

    (
        graph.build_unchecked(),
        source,
        target,
        unrelated,
        source_port,
        target_port,
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (graph, source, target, unrelated, source_port, target_port) = make_graph();
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    let edge = EdgeId::from_u128(20);
    let tx = GraphTransaction::from_ops([GraphOp::AddEdge {
        id: edge,
        edge: Edge {
            kind: EdgeKind::Data,
            from: source_port,
            to: target_port,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
    }])
    .with_label("connect source to target");

    let outcome = store.dispatch_transaction(&tx)?;
    let request =
        LayoutRequest::all().with_dirty_scope_from_footprint(store.graph(), outcome.footprint());

    let Some(nodes) = request.scope.nodes() else {
        panic!("dirty scope should be node-limited");
    };
    assert!(nodes.contains(&source));
    assert!(nodes.contains(&target));
    assert!(!nodes.contains(&unrelated));

    let layout_tx = store.dugong_layout_transaction(&request)?;
    assert!(layout_tx.ops().len() <= 2);

    if let LayoutScope::Nodes { nodes } = request.scope {
        println!(
            "dirty layout scope contains {} node(s); planned {} move op(s)",
            nodes.len(),
            layout_tx.ops().len()
        );
    }

    Ok(())
}
