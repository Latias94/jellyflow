use jellyflow_core::{CanvasPoint, CanvasSize, Graph, GraphId, Node, NodeId, NodeKindKey};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::layout::{
    LayoutContext, LayoutEngine, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest,
    LayoutError, LayoutNodePosition, LayoutRequest, LayoutResult, builtin_layout_engine_registry,
};

#[derive(Debug, Clone, Copy)]
struct RowLayoutEngine;

impl LayoutEngine for RowLayoutEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new("row")
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let nodes = graph
            .nodes
            .keys()
            .enumerate()
            .map(|(index, node)| {
                let size = context
                    .measured_node_sizes
                    .get(node)
                    .copied()
                    .or_else(|| graph.nodes.get(node).and_then(|node| node.size))
                    .unwrap_or(request.options.default_node_size);
                let pos = CanvasPoint {
                    x: index as f32 * (size.width + 48.0),
                    y: 0.0,
                };

                LayoutNodePosition {
                    node: *node,
                    pos,
                    center: CanvasPoint {
                        x: pos.x + size.width * 0.5,
                        y: pos.y + size.height * 0.5,
                    },
                    size,
                }
            })
            .collect();

        Ok(LayoutResult {
            nodes,
            edge_routes: Vec::new(),
            bounds: None,
        })
    }
}

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
            height: 72.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn make_graph() -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(1));
    graph
        .nodes
        .insert(NodeId::from_u128(2), make_node("demo.topic", 320.0, 120.0));
    graph
        .nodes
        .insert(NodeId::from_u128(3), make_node("demo.note", 640.0, 240.0));
    graph
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = NodeGraphStore::new(
        make_graph(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    let builtins = builtin_layout_engine_registry();
    let dugong_request = LayoutEngineRequest::dugong(LayoutRequest::all());
    let planned = store.plan_layout(&dugong_request, &builtins)?;
    assert_eq!(planned.nodes.len(), 2);

    let mut registry = LayoutEngineRegistry::new();
    registry.insert(RowLayoutEngine)?;
    let row_request = LayoutEngineRequest::new("row", LayoutRequest::all());
    let outcome = store.apply_layout(&row_request, &registry)?;

    println!(
        "custom layout committed {} node move(s)",
        outcome
            .committed()
            .map(|tx| tx.ops().len())
            .unwrap_or_default()
    );

    Ok(())
}
