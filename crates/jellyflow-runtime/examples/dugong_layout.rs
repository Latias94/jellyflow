use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::layout::{LayoutRequest, LayoutSpacing};

fn make_node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
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
        ports,
        data: serde_json::Value::Null,
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

fn make_graph() -> (Graph, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let source = NodeId::from_u128(2);
    let target = NodeId::from_u128(3);
    let source_port = PortId::from_u128(4);
    let target_port = PortId::from_u128(5);
    let edge = EdgeId::from_u128(6);

    graph.insert_node(source, make_node("demo.source", vec![source_port]));
    graph.insert_node(target, make_node("demo.target", vec![target_port]));
    graph.insert_port(source_port, make_port(source, "out", PortDirection::Out));
    graph.insert_port(target_port, make_port(target, "in", PortDirection::In));
    graph.insert_edge(
        edge,
        Edge {
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
    );

    (graph.build_unchecked(), source, target)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (graph, source, target) = make_graph();
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let request = LayoutRequest::all()
        .with_measured_node_sizes([
            (
                source,
                CanvasSize {
                    width: 160.0,
                    height: 64.0,
                },
            ),
            (
                target,
                CanvasSize {
                    width: 180.0,
                    height: 72.0,
                },
            ),
        ])
        .with_options(jellyflow_runtime::runtime::layout::LayoutOptions {
            spacing: LayoutSpacing {
                ranksep: 80.0,
                ..LayoutSpacing::default()
            },
            ..Default::default()
        });

    let tx = store.dugong_layout_transaction(&request)?;
    let outcome = store.dispatch_transaction(&tx)?;

    assert_eq!(outcome.committed().label(), Some("Layout graph"));
    assert_ne!(
        store.graph().nodes()[&source].pos,
        store.graph().nodes()[&target].pos
    );

    println!(
        "layout committed {} node move(s)",
        outcome.committed().ops().len()
    );

    Ok(())
}
