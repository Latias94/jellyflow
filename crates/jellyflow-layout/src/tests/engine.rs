use jellyflow_core::{CanvasPoint, CanvasSize, Graph, GraphBuilder, GraphId, GraphOp, NodeId};

use crate::{
    LayoutContext, LayoutEngine, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest,
    LayoutError, LayoutNodePosition, LayoutRequest, LayoutResult, layout_graph_with_engine,
};

#[derive(Clone)]
struct TestEngine {
    id: LayoutEngineId,
}

impl TestEngine {
    fn new(id: impl Into<LayoutEngineId>) -> Self {
        Self { id: id.into() }
    }
}

impl LayoutEngine for TestEngine {
    fn id(&self) -> LayoutEngineId {
        self.id.clone()
    }

    fn layout(
        &self,
        graph: &Graph,
        _request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let nodes = graph
            .nodes()
            .keys()
            .copied()
            .filter(|node| !context.pinned_nodes.contains(node))
            .map(|node| LayoutNodePosition {
                node,
                pos: CanvasPoint { x: 10.0, y: 20.0 },
                center: CanvasPoint { x: 60.0, y: 40.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 40.0,
                },
            })
            .collect();

        Ok(LayoutResult {
            nodes,
            edge_routes: Vec::new(),
            bounds: None,
        })
    }
}

#[test]
fn registry_resolves_custom_engine_by_stable_id() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();

    let graph = graph_with_one_node(NodeId::from_u128(1));
    let request = LayoutEngineRequest::new(engine.clone(), LayoutRequest::all());
    let result =
        layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default()).unwrap();

    assert_eq!(registry.engine_ids().collect::<Vec<_>>(), vec![&engine]);
    assert_eq!(result.nodes.len(), 1);
}

#[test]
fn registry_rejects_duplicate_engine_ids() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();

    let err = registry
        .insert(TestEngine::new(engine.clone()))
        .expect_err("duplicate engine");

    assert_eq!(err, LayoutError::DuplicateLayoutEngine(engine));
}

#[test]
fn registry_reports_missing_engine() {
    let registry = LayoutEngineRegistry::new();
    let missing = LayoutEngineId::new("missing");
    let graph = GraphBuilder::new(GraphId::from_u128(1));
    let request = LayoutEngineRequest::new(missing.clone(), LayoutRequest::all());

    let err = layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default())
        .expect_err("missing engine");

    assert_eq!(err, LayoutError::MissingLayoutEngine(missing));
}

#[test]
fn engine_request_serializes_engine_id() {
    let request = LayoutEngineRequest::new("custom.test", LayoutRequest::all());

    let encoded = serde_json::to_string(&request).expect("serialize");
    let decoded: LayoutEngineRequest = serde_json::from_str(&encoded).expect("deserialize");

    assert_eq!(decoded.engine.as_str(), "custom.test");
}

#[test]
fn engine_request_constructors_use_builtin_ids() {
    assert_eq!(
        LayoutEngineRequest::tidy_tree(LayoutRequest::all()).engine,
        LayoutEngineId::tidy_tree()
    );
}

#[test]
fn context_pinned_nodes_are_visible_to_engines() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();
    let pinned = NodeId::from_u128(1);
    let graph = graph_with_one_node(pinned);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());
    let context = LayoutContext::new().with_pinned_nodes([pinned]);

    let result = layout_graph_with_engine(&graph, &request, &registry, &context).unwrap();
    let tx = result.to_transaction(&graph).unwrap();

    assert!(result.nodes.is_empty());
    assert!(tx.is_empty());
}

#[test]
fn generic_result_converts_to_transaction() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();
    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());

    let result =
        layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default()).unwrap();
    let tx = result.to_transaction(&graph).unwrap();

    assert!(matches!(tx.ops()[0], GraphOp::SetNodePos { id, .. } if id == node));
}

fn graph_with_one_node(node: NodeId) -> Graph {
    use jellyflow_core::{Node, NodeKindKey};

    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node,
        Node {
            kind: NodeKindKey::new("demo.node"),
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
            ports: Vec::new(),
            data: serde_json::Value::Null,
        },
    );
    graph.build_unchecked()
}
